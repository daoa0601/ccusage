//! MCP (Model Context Protocol) server exposing ccusage report tools.
//!
//! Mirrors the `@ccusage/mcp` package: a `ccusage mcp` subcommand that serves
//! six tools (`daily`, `session`, `monthly`, `blocks`, `codex-daily`,
//! `codex-monthly`) over stdio (default) or HTTP transports.

pub(crate) mod server;
pub(crate) mod transport;

use crate::{cli::McpTransport, Result};

/// Entry point for `ccusage mcp`.
pub(crate) fn run(args: crate::cli::McpArgs) -> Result<()> {
    // Mirror the TS startup guard: refuse to serve when no Claude data dir exists.
    if crate::adapter::claude::claude_paths()
        .map(|paths| paths.is_empty())
        .unwrap_or(true)
    {
        return Err(crate::cli_error(
            "No valid Claude data directory found",
        ));
    }

    match args.transport {
        McpTransport::Stdio => transport::serve_stdio(&args),
        McpTransport::Http => transport::serve_http(&args),
    }
}
