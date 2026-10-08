//! telegram-mcp: Telegram MCP server entry point.
//!
//! Original from telegram-mcp (MIT, (c) 2026 septagram).
//! Optimized: merged duplicated serve loops into one.

mod auth;
mod mcp;
mod setup;
mod telegram;
// Explicit path: the legacy `src/tools.rs` monolith may still exist on disk;
// the modular `src/tools/` tree is authoritative.
#[path = "tools/mod.rs"]
mod tools;

use grammers_client::Client;
use mcp::{
    CallToolResult, InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities,
    ServerInfo, ToolsCapability, ToolsListResult,
};
use serde_json::Value;
use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, error, info, warn};

fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--register") => return setup::register_mcp(),
        Some("--unregister") => return setup::unregister_mcp(),
        Some("--auth") => {
            let rt = tokio::runtime::Runtime::new()?;
            return rt.block_on(auth::interactive_auth());
        }
        _ => {}
    }
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(serve())
}

async fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tg_mcp_monitor=info".parse().unwrap()),
        )
        .init();

    info!("Starting telegram-mcp server");

    // Connect to Telegram (optional — server still lists tools without it).
    let client: Option<Client> = match auth::connect().await {
        Ok((client, _pool_task)) => {
            info!("Connected to Telegram");
            Some(client)
        }
        Err(e) => {
            warn!("Failed to connect to Telegram: {e:#}");
            warn!("Tools will return errors until authentication is set up.");
            warn!("Run `telegram-mcp --auth` to authenticate.");
            None
        }
    };

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        debug!(raw = %line, "Received");

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {e}"));
                send_response(&resp);
                continue;
            }
        };

        if let Some(resp) = handle_request(client.as_ref(), &request).await {
            send_response(&resp);
        }
    }

    info!("stdin closed, shutting down");
    Ok(())
}

async fn handle_request(
    client: Option<&Client>,
    req: &JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    let id = req.id.clone();

    match req.method.as_str() {
        // ── Lifecycle ──────────────────────────────────────────────
        "initialize" => {
            let result = InitializeResult {
                protocol_version: "2024-11-05".into(),
                capabilities: ServerCapabilities {
                    tools: ToolsCapability {},
                },
                server_info: ServerInfo {
                    name: "telegram-mcp".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                },
            };
            Some(JsonRpcResponse::success(
                id,
                serde_json::to_value(result).unwrap(),
            ))
        }

        "notifications/initialized" => {
            info!("Client initialized");
            None
        }

        // ── Tools ──────────────────────────────────────────────────
        "tools/list" => {
            let result = ToolsListResult {
                tools: tools::all_tool_definitions(),
            };
            Some(JsonRpcResponse::success(
                id,
                serde_json::to_value(result).unwrap(),
            ))
        }

        "tools/call" => {
            let params = req.params.as_ref().cloned().unwrap_or(Value::Null);
            let tool_name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            info!(tool = %tool_name, "Tool call");

            let Some(client) = client else {
                return Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(CallToolResult::err(
                        "Error: Not connected to Telegram. Run `telegram-mcp --auth` to authenticate, then restart the client.".into(),
                    ))
                    .unwrap(),
                ));
            };

            match tools::dispatch_tool_call(client, tool_name, &arguments).await {
                Ok(result) => Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(result).unwrap(),
                )),
                Err(e) => {
                    error!(tool = %tool_name, error = %e, "Tool call failed");
                    Some(JsonRpcResponse::success(
                        id,
                        serde_json::to_value(CallToolResult::err(format!("Error: {e:#}"))).unwrap(),
                    ))
                }
            }
        }

        // ── Unknown ────────────────────────────────────────────────
        method => {
            debug!(method, "Unknown method");
            if id.is_none() || id.as_ref() == Some(&Value::Null) {
                None
            } else {
                Some(JsonRpcResponse::error(
                    id,
                    -32601,
                    format!("Method not found: {method}"),
                ))
            }
        }
    }
}

fn send_response(resp: &JsonRpcResponse) {
    let json = serde_json::to_string(resp).expect("Failed to serialize response");
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{json}").expect("Failed to write to stdout");
    stdout.flush().expect("Failed to flush stdout");
    debug!(json = %json, "Sent");
}
