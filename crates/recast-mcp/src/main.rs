//! `recast-mcp` — Model Context Protocol server exposing recast's
//! rewrite engine to MCP-aware agents.
//!
//! Speaks JSON-RPC over stdio per MCP convention. Bundles `recast-core`
//! as a library — no subprocess hop, no CLI string assembly on the
//! agent side, no version skew between client and engine.

mod server;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use crate::server::RecastServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr so they don't pollute the JSON-RPC framing on
    // stdout that the MCP client is parsing.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("recast-mcp starting");

    let service = RecastServer::new().serve(stdio()).await.inspect_err(|e| {
        tracing::error!("rmcp serve failed: {e:?}");
    })?;

    service.waiting().await?;
    Ok(())
}
