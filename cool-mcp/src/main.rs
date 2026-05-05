//! cool-mcp — MCP server exposing NTU COOL tools to AI assistants.
//!
//! Designed to be launched by an MCP client (Claude Desktop / Cursor / etc.)
//! over stdio. Reuses `cool-api` for auth (existing session.json) and
//! `cool-tools` for the actual tool logic.

use std::sync::Arc;

use anyhow::Result;
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

#[derive(Clone)]
struct CoolServer {
    client: Arc<CoolClient>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CoursesListArgs {
    /// If true, include past/inactive enrolments (defaults to active only)
    #[serde(default)]
    all: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CoursesResolveArgs {
    /// Course name substring or numeric ID
    query: String,
}

#[tool_router]
impl CoolServer {
    fn new(client: CoolClient) -> Self {
        Self {
            client: Arc::new(client),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Show the currently logged-in NTU COOL user's profile")]
    async fn whoami(&self) -> Result<CallToolResult, ErrorData> {
        let profile = cool_tools::profile::whoami(&self.client)
            .await
            .map_err(to_mcp_err)?;
        json_result(&profile)
    }

    #[tool(
        description = "List the user's enrolled courses. Returns id, name, course_code. By default returns only active enrolments."
    )]
    async fn courses_list(
        &self,
        Parameters(args): Parameters<CoursesListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let courses = if args.all {
            cool_tools::courses::list_all(&self.client, true).await
        } else {
            cool_tools::courses::list_active(&self.client).await
        }
        .map_err(to_mcp_err)?;
        json_result(&courses)
    }

    #[tool(
        description = "Resolve a course query (numeric ID or name/code substring) to one or more course matches. Returns a list of {id, name, course_code} — empty if no match, multiple if ambiguous."
    )]
    async fn courses_resolve(
        &self,
        Parameters(args): Parameters<CoursesResolveArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let matches = cool_tools::courses::resolve(&self.client, &args.query)
            .await
            .map_err(to_mcp_err)?;
        json_result(&matches)
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

    tracing::info!("cool-mcp starting on stdio");

    let server = CoolServer::new(client);
    let running = server.serve(stdio()).await?;
    let reason = running.waiting().await?;
    tracing::info!(?reason, "cool-mcp exited");
    Ok(())
}
