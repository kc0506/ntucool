//! Contract types — the public, AI-facing shape of every tool's I/O.
//!
//! These structs are the contract documented in `docs/TOOLS.md`. They are
//! **deliberately trimmed** — Canvas API responses carry 25+ fields per object,
//! 90% of which are noise for an AI consumer. Adding a field here is a
//! deliberate API decision; raw Canvas types stay inside `cool-api` /
//! `cool-tools` internals and never leak through MCP.
//!
//! Round-trip note: every type derives `Serialize`/`Deserialize`/`JsonSchema`
//! so cool-mcp can hand them straight to the rmcp `Parameters<>` extractor and
//! `json_result()`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// Profile
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProfileSummary {
    pub id: i64,
    pub name: String,
    pub login_id: Option<String>,
    pub primary_email: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Course
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CourseSummary {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
    /// Human-readable term name when known (e.g. "112-1"). `None` if Canvas did
    /// not include term data on this listing.
    pub term: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolveMatch {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
    /// Fuzzy match confidence 0.0..=1.0. 1.0 = exact id, ~0.9 = exact substring,
    /// lower = partial. Numeric IDs always score 1.0 even without API hit.
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CourseDetail {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
    pub term: Option<String>,
    /// Canvas syllabus body (HTML). Caller decides whether to convert to text.
    pub syllabus_html: Option<String>,
    pub teachers: Vec<TeacherSummary>,
    pub default_view: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeacherSummary {
    pub id: i64,
    pub name: String,
}

// ────────────────────────────────────────────────────────────────────────────
// File
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileSummary {
    pub id: i64,
    pub display_name: String,
    pub size_bytes: i64,
    pub mime_type: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileMetadata {
    pub id: i64,
    pub display_name: String,
    pub size_bytes: i64,
    pub mime_type: Option<String>,
    pub updated_at: Option<String>,
    /// Canvas-signed download URL. Time-limited; treat as opaque.
    pub url: Option<String>,
    pub folder_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FolderListing {
    pub course_id: i64,
    /// Path the listing represents, "/" for root.
    pub path: String,
    pub folders: Vec<FolderSummary>,
    pub files: Vec<FileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FolderSummary {
    pub id: i64,
    pub name: String,
    pub files_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DownloadResult {
    pub file_id: i64,
    pub dest_path: String,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilesFetchResult {
    pub file_id: i64,
    pub display_name: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    /// URI to access the file's bytes. Scheme depends on the cool-mcp
    /// instance's serve mode:
    ///   stdio mode → `file:///abs/path` to a server-internal cache file
    ///   http mode  → `https://host/cache/<token>.ext` (not implemented yet)
    /// Pass to user, or read via this URI directly. Cache-controlled — repeat
    /// calls reuse the same URI while Canvas's `updated_at` is unchanged.
    pub uri: String,
    /// ISO-8601 expiry. `None` for `file://` (no per-URI expiry; subject
    /// only to cache eviction). Always set for HTTP URIs.
    pub expires_at: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Assignment
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssignmentSummary {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub due_at: Option<String>,
    pub points_possible: Option<f64>,
    /// Canvas bucket on listing context (`upcoming` / `past` / etc), if filtered.
    pub bucket: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssignmentDetail {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    /// Plain-text version of Canvas assignment description (HTML stripped).
    pub description_text: Option<String>,
    pub due_at: Option<String>,
    pub points_possible: Option<f64>,
    pub submission_types: Vec<String>,
    pub html_url: Option<String>,
    /// Rubric criteria when set. Empty Vec when no rubric.
    pub rubric: Vec<RubricCriterion>,
    /// File references extracted from the assignment description's HTML
    /// (`<a href=".../files/{id}">`). Canvas does not return a typed
    /// `attachments` array on assignments. Use `files.get` with the `id`
    /// to fetch full metadata (size, mime, signed URL).
    pub attachments: Vec<AttachmentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentRef {
    pub id: i64,
    /// Visible link text (usually the original filename).
    pub name: String,
    /// HTML `href` of the link.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RubricCriterion {
    pub description: String,
    pub points: f64,
    pub long_description: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Announcement
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementSummary {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    pub posted_at: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementDetail {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    /// Plain-text body (HTML stripped).
    pub body_text: String,
    pub posted_at: Option<String>,
    pub html_url: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Module
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleSummary {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub position: Option<i64>,
    pub items_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleDetail {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub position: Option<i64>,
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleItem {
    pub id: i64,
    pub title: String,
    /// File / Page / Discussion / Assignment / Quiz / ExternalUrl / ExternalTool / SubHeader.
    pub item_type: String,
    /// Resource ID for File/Page/etc; `None` for SubHeader and ExternalUrl.
    pub content_id: Option<i64>,
    pub url: Option<String>,
    pub position: Option<i64>,
    pub indent: Option<i64>,
}

// ────────────────────────────────────────────────────────────────────────────
// Discussion
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscussionSummary {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    pub posted_at: Option<String>,
    pub author_name: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscussionDetail {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    pub message_text: String,
    pub posted_at: Option<String>,
    pub author_name: Option<String>,
    pub html_url: Option<String>,
    /// Top-level entries (first level of replies). Empty when caller did not
    /// request `with_entries`.
    pub entries: Vec<DiscussionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscussionEntry {
    pub id: i64,
    pub author_name: Option<String>,
    pub message_text: String,
    pub posted_at: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Page (Canvas wiki)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageSummary {
    pub course_id: i64,
    /// URL slug — Canvas's primary key for pages within a course.
    pub url: String,
    pub title: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageDetail {
    pub course_id: i64,
    pub url: String,
    pub title: String,
    /// Plain-text body (HTML stripped).
    pub body_text: String,
    pub updated_at: Option<String>,
    pub html_url: Option<String>,
}
