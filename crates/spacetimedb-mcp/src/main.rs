//! MCP server for SpacetimeDB.
//!
//! Transport is stdio: the process is launched as a subprocess by an MCP client,
//! speaks JSON-RPC over stdin/stdout, and logs to stderr only. Nothing else may
//! touch stdout or the protocol stream is corrupted.

mod introspect;
mod stdb;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ErrorData, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::Deserialize;

#[derive(Clone)]
struct SpacetimeDbMcp {
    // Required by the `#[tool_router]`/`#[tool_handler]` macro pattern: the
    // router is built in `new` and consumed by the generated handler. rustc's
    // dead-code pass can't see the macro-internal use, hence the allow.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Surface an internal error to the MCP client as a tool error.
fn to_mcp_error(err: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(err.to_string(), None)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PingParams {
    /// Optional message echoed back, to confirm round-trip works.
    message: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DatabaseParams {
    /// Name or identity of the target database, as known to the SpacetimeDB host.
    database: String,
}

#[tool_router]
impl SpacetimeDbMcp {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Health check. Echoes back an optional message to confirm the server is alive.")]
    async fn ping(&self, Parameters(p): Parameters<PingParams>) -> String {
        match p.message {
            Some(m) => format!("pong: {m}"),
            None => "pong".to_string(),
        }
    }

    #[tool(
        description = "Fetch the full schema (module definition) of a database as JSON: typespace, tables, and reducers."
    )]
    async fn get_schema(&self, Parameters(p): Parameters<DatabaseParams>) -> Result<String, ErrorData> {
        let def = stdb::Client::from_env()
            .module_def(&p.database)
            .await
            .map_err(to_mcp_error)?;
        introspect::schema_json(&def).map_err(|e| to_mcp_error(e.into()))
    }

    #[tool(description = "List the names of all tables defined in a database.")]
    async fn list_tables(&self, Parameters(p): Parameters<DatabaseParams>) -> Result<String, ErrorData> {
        let def = stdb::Client::from_env()
            .module_def(&p.database)
            .await
            .map_err(to_mcp_error)?;
        serde_json::to_string_pretty(&introspect::table_names(&def)).map_err(|e| to_mcp_error(e.into()))
    }

    #[tool(
        description = "List the reducers in a database, with each reducer's lifecycle role (init, on_connect, on_disconnect) when it has one."
    )]
    async fn list_reducers(&self, Parameters(p): Parameters<DatabaseParams>) -> Result<String, ErrorData> {
        let def = stdb::Client::from_env()
            .module_def(&p.database)
            .await
            .map_err(to_mcp_error)?;
        serde_json::to_string_pretty(&introspect::reducer_summaries(&def)).map_err(|e| to_mcp_error(e.into()))
    }
}

#[tool_handler]
impl ServerHandler for SpacetimeDbMcp {
    fn get_info(&self) -> ServerInfo {
        // ServerInfo is #[non_exhaustive]; build from Default (which fills
        // server_info name/version from this crate via from_build_env) and
        // override only what we need.
        let mut info = ServerInfo::default();
        // Default's from_build_env() reports rmcp's own name/version; override
        // with ours so clients identify this server correctly.
        info.server_info.name = env!("CARGO_PKG_NAME").into();
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "MCP server for SpacetimeDB. Introspect a database's schema, tables, and reducers \
             against a running SpacetimeDB host (set SPACETIMEDB_HOST, and SPACETIMEDB_TOKEN \
             for private databases)."
                .into(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs MUST go to stderr; stdout is reserved for the JSON-RPC stream.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("starting spacetimedb-mcp on stdio");
    let service = SpacetimeDbMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
