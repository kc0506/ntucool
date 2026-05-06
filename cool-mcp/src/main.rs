//! cool-mcp — MCP server exposing NTU COOL tools to AI assistants.
//!
//! Designed to be launched by an MCP client (Claude Desktop / Cursor / etc.)
//! over stdio. Reuses `cool-api` for auth (existing session.json) and
//! `cool-tools` for the actual tool logic.
//!
//! Every tool returns a contract-shape struct from `cool_tools::types` —
//! deliberately trimmed compared to raw Canvas responses. See `docs/TOOLS.md`
//! for the spec; if a tool description and `cool_tools::types` shape disagree,
//! the spec wins.

mod file_server;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars,
    service::ServiceExt,
    tool, tool_handler, tool_router,
    transport::io::stdio,
    ErrorData, ServerHandler,
};
use serde::Deserialize;

use cool_api::CoolClient;

use crate::file_server::{FileServer, HttpPublisher, StdioPublisher};

#[derive(Clone)]
struct CoolServer {
    client: Arc<CoolClient>,
    /// Server-internal cache directory. Files are downloaded here on
    /// `files_fetch`; never exposed to the client.
    server_cache_dir: PathBuf,
    /// Server-internal text cache for `pdf_extract` / `pdf_search`. JSON
    /// sidecar per file, keyed identically to the bytes cache.
    text_cache_dir: PathBuf,
    /// Public-facing publisher: turns a server-internal `CachedFile` into
    /// a URI the client can read.
    file_server: Arc<FileServer>,
    tool_router: ToolRouter<Self>,
}

// ────────────────────────────────────────────────────────────────────────────
// Argument structs (Tier 0 / 1)
//
// Naming: `id` alone is ambiguous in a multi-arg context, so each resource's
// id is qualified (`assignment_id`, `topic_id`, …). Spec uses `id`; the
// qualified names are an intentional contract refinement for AI ergonomics.
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CoursesListArgs {
    /// "active" (default) or "all" — controls whether past/inactive enrolments are included.
    #[serde(default)]
    filter: Option<String>,
    /// Substring of term name or sis_term_id (e.g. "112-1"). Only honoured when filter="all".
    #[serde(default)]
    term: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CoursesResolveArgs {
    /// Course name substring, course_code, or numeric ID
    query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CoursesGetArgs {
    course_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FilesListArgs {
    course_id: i64,
    /// Folder path within the course, "/"-separated. Defaults to course root.
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FilesSearchArgs {
    /// Optional course scope. When omitted, searches across every course
    /// the user has access to via `/api/v1/users/self/files`.
    #[serde(default)]
    course_id: Option<i64>,
    /// Query string. Canvas requires at least 3 bytes; shorter queries error.
    query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FilesGetMetadataArgs {
    /// Global Canvas file ID. Resolved via `/api/v1/files/:id` — no course scope needed.
    file_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FilesFetchArgs {
    /// Global Canvas file ID.
    file_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AssignmentsListArgs {
    course_id: i64,
    /// Canvas server-side state filter. Values: upcoming (next 7 days, unsubmitted),
    /// future (>7 days out), overdue, past (submitted), undated, ungraded, unsubmitted.
    /// Omit to return everything.
    #[serde(default)]
    bucket: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AssignmentsGetArgs {
    course_id: i64,
    assignment_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AnnouncementsListArgs {
    /// Course IDs to scope to. Empty list = all currently-active enrolments.
    #[serde(default)]
    course_ids: Vec<i64>,
    /// ISO-8601 timestamp; only announcements posted at or after this time are returned.
    #[serde(default)]
    since: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AnnouncementsGetArgs {
    course_id: i64,
    /// Discussion topic ID (announcements are stored as discussion topics in Canvas)
    topic_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ModulesListArgs {
    course_id: i64,
    /// When true, return each module's items inline so you don't have to call
    /// `modules_get` per module just to find where a resource lives. Default false.
    #[serde(default)]
    with_items: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ModulesGetArgs {
    course_id: i64,
    module_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiscussionsListArgs {
    course_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiscussionsGetArgs {
    course_id: i64,
    topic_id: i64,
    /// Include first-level entries (replies). Default true.
    #[serde(default = "default_true")]
    with_entries: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PagesListArgs {
    course_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PagesGetArgs {
    course_id: i64,
    /// Canvas page URL slug — the primary key for pages within a course.
    url: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PdfExtractArgs {
    file_id: i64,
    /// Page selector. Forms: "all" (or omit), "5", "5-10". 1-indexed.
    /// Omit only when you actually want every page — large textbooks can
    /// blow the AI client's context budget.
    #[serde(default)]
    pages: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PdfSearchArgs {
    course_id: i64,
    /// Case-insensitive substring. Whitespace is matched literally.
    query: String,
    /// Cap total hits across all PDFs. Defaults to 20.
    #[serde(default)]
    max_results: Option<usize>,
}

// ────────────────────────────────────────────────────────────────────────────
// Tool router
// ────────────────────────────────────────────────────────────────────────────

#[tool_router]
impl CoolServer {
    fn new(
        client: CoolClient,
        server_cache_dir: PathBuf,
        text_cache_dir: PathBuf,
        file_server: FileServer,
    ) -> Self {
        Self {
            client: Arc::new(client),
            server_cache_dir,
            text_cache_dir,
            file_server: Arc::new(file_server),
            tool_router: Self::tool_router(),
        }
    }

    // ── Tier 0 ─────────────────────────────────────────────────────────────

    #[tool(description = "Show the currently logged-in NTU COOL user. Returns ProfileSummary {id, name, login_id, primary_email}.")]
    async fn whoami(&self) -> Result<CallToolResult, ErrorData> {
        let profile = cool_tools::profile::whoami_summary(&self.client)
            .await
            .map_err(to_mcp_err)?;
        json_result(&profile)
    }

    #[tool(description = "List enrolled courses. Returns [CourseSummary {id, name, course_code, term}]. \
        filter='active' (default) returns currently enrolled courses; filter='all' \
        includes past/inactive. term filters by enrolment_term name substring (only when filter='all').")]
    async fn courses_list(
        &self,
        Parameters(args): Parameters<CoursesListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let filter = match args.filter.as_deref().unwrap_or("active") {
            "all" => cool_tools::courses::ListFilter::All,
            _ => cool_tools::courses::ListFilter::Active,
        };
        let summaries =
            cool_tools::courses::list_summaries(&self.client, filter, args.term.as_deref())
                .await
                .map_err(to_mcp_err)?;
        json_result(&summaries)
    }

    #[tool(description = "Resolve a course query (name/course_code substring or numeric ID) to one or \
        more matches. Returns [ResolveMatch {id, name, course_code, score}] sorted by score (desc); 1.0 = exact id.")]
    async fn courses_resolve(
        &self,
        Parameters(args): Parameters<CoursesResolveArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let matches = cool_tools::courses::resolve_with_score(&self.client, &args.query)
            .await
            .map_err(to_mcp_err)?;
        json_result(&matches)
    }

    #[tool(description = "Get a course's syllabus, term, and teachers. Returns CourseDetail.")]
    async fn courses_get(
        &self,
        Parameters(args): Parameters<CoursesGetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let detail = cool_tools::courses::get_detail(&self.client, args.course_id)
            .await
            .map_err(to_mcp_err)?;
        json_result(&detail)
    }

    // ── Tier 1: files ──────────────────────────────────────────────────────

    #[tool(description = "List a course folder's contents. Returns FolderListing {course_id, path, folders[], files[]}. \
        path defaults to course root (\"/\").")]
    async fn files_list(
        &self,
        Parameters(args): Parameters<FilesListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let listing = cool_tools::files::list_in_course_summary(
            &self.client,
            args.course_id,
            args.path.as_deref(),
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&listing)
    }

    #[tool(description = "Filename search via Canvas search_term. course_id is optional: omit to \
        search across every course the user has access to. Returns [FileSummary]. Query must be at \
        least 3 bytes — Canvas rejects shorter queries with HTTP 400.")]
    async fn files_search(
        &self,
        Parameters(args): Parameters<FilesSearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let files =
            cool_tools::files::search_summaries(&self.client, args.course_id, &args.query)
                .await
                .map_err(to_mcp_err)?;
        json_result(&files)
    }

    #[tool(description = "Get a single file's metadata via the global /api/v1/files/:id endpoint. \
        Returns FileMetadata {id, display_name, size_bytes, mime_type, updated_at, url, folder_id}. \
        Note: the `url` field is Canvas's internal path that requires the user's browser session — \
        for AI/cross-machine bytes access use `files_fetch` instead.")]
    async fn files_get_metadata(
        &self,
        Parameters(args): Parameters<FilesGetMetadataArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let meta = cool_tools::files::get_metadata_global(&self.client, args.file_id)
            .await
            .map_err(to_mcp_err)?;
        json_result(&meta)
    }

    #[tool(description = "Fetch a file's bytes through cool-mcp. Returns FilesFetchResult \
        {file_id, display_name, mime_type, size_bytes, uri, expires_at?}. The URI scheme depends \
        on the server's serve mode — independent of, and never revealing, the server's internal cache:\n\
        - stdio mode → `file://<output_dir>/<file_id>/<display_name>` to a copy under the publish \
          dir (configurable via COOL_MCP_OUTPUT_DIR; default $XDG_DATA_HOME/cool-mcp/files). The \
          output is decoupled from the server's internal cache so the client can read/move/delete \
          safely.\n\
        - http mode  → `http://host:port/files/<token>` short-lived signed URL backed by the \
          server's internal cache. `expires_at` is set in this mode (TTL via COOL_MCP_HTTP_TTL_SECS, \
          default 3600s). Repeat calls reuse the same token while it's still alive.\n\
        Cache-controlled — Canvas's updated_at gates re-downloads.")]
    async fn files_fetch(
        &self,
        Parameters(args): Parameters<FilesFetchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let cached = cool_tools::files::cache_or_download(
            &self.client,
            args.file_id,
            &self.server_cache_dir,
        )
        .await
        .map_err(to_mcp_err)?;
        let result = self
            .file_server
            .publish(cached)
            .await
            .map_err(to_mcp_err)?;
        json_result(&result)
    }

    // ── Tier 1: assignments ────────────────────────────────────────────────

    #[tool(description = "List assignments for a course. Returns [AssignmentSummary]. \
        bucket is a Canvas server-side state filter:\n\
        - upcoming: due in the next 7 days, not yet submitted\n\
        - future: due more than 7 days out\n\
        - overdue: past due_at, not submitted\n\
        - past: already submitted\n\
        - undated: no due_at set\n\
        - ungraded: needs grading\n\
        - unsubmitted: no submission yet (regardless of due date)\n\
        Note the 7-day cutoff between `upcoming` and `future` — for \"what's due soon\" \
        queries spanning >1 week, omit bucket entirely or query both.")]
    async fn assignments_list(
        &self,
        Parameters(args): Parameters<AssignmentsListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let assignments = cool_tools::assignments::list_summaries(
            &self.client,
            args.course_id,
            args.bucket.as_deref(),
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&assignments)
    }

    #[tool(description = "Get one assignment's full description (HTML→text), due date, points, submission types, \
        rubric, and attachments (file refs mined from the description HTML). Canvas requires course_id \
        because /api/v1/assignments/:id is not exposed (404).")]
    async fn assignments_get(
        &self,
        Parameters(args): Parameters<AssignmentsGetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let detail = cool_tools::assignments::get_detail(
            &self.client,
            args.course_id,
            args.assignment_id,
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&detail)
    }

    // ── Tier 1: announcements ──────────────────────────────────────────────

    #[tool(description = "List announcements across one or more courses. course_ids defaults to []; \
        when empty, all currently-active enrolments are used. since filters by ISO-8601 posted_at threshold.")]
    async fn announcements_list(
        &self,
        Parameters(args): Parameters<AnnouncementsListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let announcements = cool_tools::announcements::list_summaries(
            &self.client,
            &args.course_ids,
            args.since.as_deref(),
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&announcements)
    }

    #[tool(description = "Get one announcement's body (HTML→text). Canvas requires course_id alongside \
        topic_id because /api/v1/discussion_topics/:id is not exposed (404).")]
    async fn announcements_get(
        &self,
        Parameters(args): Parameters<AnnouncementsGetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let detail = cool_tools::announcements::get_detail(
            &self.client,
            args.course_id,
            args.topic_id,
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&detail)
    }

    // ── Tier 1: modules ────────────────────────────────────────────────────

    #[tool(description = "List a course's modules. with_items=false (default) returns [ModuleSummary] \
        — id/name/position/items_count only. with_items=true returns [ModuleDetail] — same plus \
        every module's items {id, title, item_type, content_id, url, position, indent} inline, \
        in one paginated call. Prefer with_items=true when you'd otherwise iterate modules_get \
        across the course.")]
    async fn modules_list(
        &self,
        Parameters(args): Parameters<ModulesListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        if args.with_items {
            let tree = cool_tools::modules::list_tree(&self.client, args.course_id)
                .await
                .map_err(to_mcp_err)?;
            json_result(&tree)
        } else {
            let modules = cool_tools::modules::list_summaries(&self.client, args.course_id)
                .await
                .map_err(to_mcp_err)?;
            json_result(&modules)
        }
    }

    #[tool(description = "Get one module with its items (type, content_id, url). Canvas requires course_id \
        because /api/v1/modules/:id is not exposed (404).")]
    async fn modules_get(
        &self,
        Parameters(args): Parameters<ModulesGetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let detail =
            cool_tools::modules::get_detail(&self.client, args.course_id, args.module_id)
                .await
                .map_err(to_mcp_err)?;
        json_result(&detail)
    }

    // ── Tier 1: discussions ────────────────────────────────────────────────

    #[tool(description = "List a course's discussion topics (excluding announcements). Returns [DiscussionSummary].")]
    async fn discussions_list(
        &self,
        Parameters(args): Parameters<DiscussionsListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let topics = cool_tools::discussions::list_summaries(&self.client, args.course_id)
            .await
            .map_err(to_mcp_err)?;
        json_result(&topics)
    }

    #[tool(description = "Get one discussion topic. with_entries=true (default) also fetches first-level \
        entries. Canvas requires course_id because /api/v1/discussion_topics/:id is not exposed (404).")]
    async fn discussions_get(
        &self,
        Parameters(args): Parameters<DiscussionsGetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let detail = cool_tools::discussions::get_detail(
            &self.client,
            args.course_id,
            args.topic_id,
            args.with_entries,
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&detail)
    }

    // ── Tier 1: pages ──────────────────────────────────────────────────────

    #[tool(description = "List a course's wiki pages. Returns [PageSummary {course_id, url, title, updated_at}].")]
    async fn pages_list(
        &self,
        Parameters(args): Parameters<PagesListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let pages = cool_tools::pages::list_summaries(&self.client, args.course_id)
            .await
            .map_err(to_mcp_err)?;
        json_result(&pages)
    }

    #[tool(description = "Get one wiki page by its URL slug. Returns PageDetail with body_text (HTML→text).")]
    async fn pages_get(
        &self,
        Parameters(args): Parameters<PagesGetArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let detail = cool_tools::pages::get_detail(&self.client, args.course_id, &args.url)
            .await
            .map_err(to_mcp_err)?;
        json_result(&detail)
    }

    // ── Tier 2: PDF extract & content search ──────────────────────────────

    #[tool(description = "Extract per-page text from a PDF in NTU COOL. Returns PdfExtractResult \
        {file_id, display_name, page_count, pages: [{page_no, text}], empty}. \
        `pages` arg selects a 1-indexed range: \"all\" (whole document), \"5\" (single page), \
        \"5-10\" (range, inclusive). Omitting `pages` returns every page — fine for short docs, \
        but cap with a range for textbooks. \
        Text is extracted once via pdf-extract and cached on disk keyed by Canvas's updated_at, \
        so repeat calls are sub-second. `empty=true` indicates an image-only or encrypted PDF.")]
    async fn pdf_extract(
        &self,
        Parameters(args): Parameters<PdfExtractArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let range = match args.pages.as_deref() {
            Some(s) => Some(cool_tools::pdf::PageRange::parse(s).map_err(to_mcp_err)?),
            None => None,
        };
        let result = cool_tools::pdf::extract(
            &self.client,
            args.file_id,
            range.as_ref(),
            &self.server_cache_dir,
            &self.text_cache_dir,
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&result)
    }

    #[tool(description = "Case-insensitive substring search across every PDF in a course. \
        Returns [PdfSearchHit {file_id, display_name, page, snippet}]. \
        First call against a course has to download + extract every PDF (slow — minutes for \
        a 50-PDF course); subsequent calls reuse the on-disk text cache and complete in \
        seconds. Limit results with max_results (default 20). To then read more context \
        around a hit, call pdf_extract with the same file_id and pages=\"<page>-<page+2>\".")]
    async fn pdf_search(
        &self,
        Parameters(args): Parameters<PdfSearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let max = args.max_results.unwrap_or(20);
        let hits = cool_tools::pdf::search_in_course(
            &self.client,
            args.course_id,
            &args.query,
            max,
            &self.server_cache_dir,
            &self.text_cache_dir,
        )
        .await
        .map_err(to_mcp_err)?;
        json_result(&hits)
    }
}

#[tool_handler]
impl ServerHandler for CoolServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn to_mcp_err(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(format!("{e:#}"), None)
}

// ────────────────────────────────────────────────────────────────────────────
// Bootstrap helpers
// ────────────────────────────────────────────────────────────────────────────

fn xdg_cache_home() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".cache")
        })
}

/// Server-internal cache directory. Holds raw bytes downloaded from Canvas;
/// never exposed in any URI returned to the client.
fn server_cache_dir() -> PathBuf {
    xdg_cache_home().join("cool-mcp").join("cache")
}

/// Server-internal cache for PDF text extraction. JSON sidecars keyed
/// identically to the bytes cache.
fn server_text_cache_dir() -> PathBuf {
    xdg_cache_home().join("cool-mcp").join("text")
}

/// stdio mode default output dir (where `files_fetch` puts a copy a client
/// can read). Distinct from `server_cache_dir`. Override via `COOL_MCP_OUTPUT_DIR`.
fn default_stdio_output_dir() -> PathBuf {
    if let Ok(v) = std::env::var("COOL_MCP_OUTPUT_DIR") {
        return PathBuf::from(v);
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".local").join("share")
        });
    base.join("cool-mcp").join("files")
}

/// Build the publisher requested by env. Async because `HttpPublisher::start`
/// has to bind a TCP socket.
async fn build_file_server() -> Result<FileServer> {
    let mode = std::env::var("COOL_MCP_FILE_MODE").unwrap_or_else(|_| "stdio".into());
    match mode.as_str() {
        "stdio" => {
            let output = default_stdio_output_dir();
            let publisher = StdioPublisher::new(output)?;
            Ok(FileServer::Stdio(publisher))
        }
        "http" => {
            let bind: SocketAddr = std::env::var("COOL_MCP_HTTP_BIND")
                .unwrap_or_else(|_| "127.0.0.1:0".into())
                .parse()
                .context("COOL_MCP_HTTP_BIND must be a SocketAddr like 127.0.0.1:31337")?;
            let public_base = std::env::var("COOL_MCP_HTTP_PUBLIC_BASE").ok();
            let ttl_secs: u64 = std::env::var("COOL_MCP_HTTP_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600);
            let publisher =
                HttpPublisher::start(bind, public_base, Duration::from_secs(ttl_secs)).await?;
            Ok(FileServer::Http(publisher))
        }
        other => anyhow::bail!("unknown COOL_MCP_FILE_MODE={other}; expected stdio|http"),
    }
}

fn json_result<T: serde::Serialize>(value: &T) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| ErrorData::internal_error(format!("serialize: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Log to stderr only — stdout is reserved for the MCP transport.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let client = CoolClient::from_default_session()
        .map_err(|e| anyhow::anyhow!("No valid session ({e}). Run `cool login` first."))?;

    let cache_dir = server_cache_dir();
    let text_cache_dir = server_text_cache_dir();
    std::fs::create_dir_all(&cache_dir).ok();
    std::fs::create_dir_all(&text_cache_dir).ok();

    let file_server = build_file_server().await?;
    tracing::info!(
        cache_dir = %cache_dir.display(),
        text_cache_dir = %text_cache_dir.display(),
        publisher = %file_server.describe(),
        "cool-mcp starting on stdio"
    );

    let server = CoolServer::new(client, cache_dir, text_cache_dir, file_server);
    let running = server.serve(stdio()).await?;
    let reason = running.waiting().await?;
    tracing::info!(?reason, "cool-mcp exited");
    Ok(())
}
