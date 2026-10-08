//! MCP tool modules.
//!
//! Each submodule defines its tools and handlers.
//! Aggregated by `super::tool_definitions()` and `super::handle_tool_call()`.

pub mod accounts;
pub mod chats;
pub mod contacts;
pub mod events;
pub mod folders;
pub mod groups;
pub mod media;
pub mod messages;
pub mod profile;

use crate::mcp::{CallToolResult, ToolDefinition};
use grammers_client::Client;
use serde_json::Value;

/// Get a string argument, defaulting to "".
pub fn str_arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// Get a usize argument with default.
pub fn usize_arg(args: &Value, key: &str, default: u64) -> usize {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default) as usize
}

/// Get an i64 argument with default.
pub fn i64_arg(args: &Value, key: &str, default: i64) -> i64 {
    args.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

/// Get a bool argument with default.
pub fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Aggregate all tool definitions from submodules.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    let mut defs = Vec::new();
    defs.extend(accounts::tool_definitions());
    defs.extend(chats::tool_definitions());
    defs.extend(contacts::tool_definitions());
    defs.extend(events::tool_definitions());
    defs.extend(folders::tool_definitions());
    defs.extend(groups::tool_definitions());
    defs.extend(media::tool_definitions());
    defs.extend(messages::tool_definitions());
    defs.extend(profile::tool_definitions());
    defs
}

/// Dispatch a tool call to the appropriate submodule.
pub async fn dispatch_tool_call(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<CallToolResult> {
    // Try each module in order
    if let Some(result) = accounts::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = chats::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = contacts::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = events::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = folders::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = groups::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = media::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = messages::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    if let Some(result) = profile::try_handle(client, tool_name, args).await? {
        return Ok(result);
    }
    anyhow::bail!("Unknown tool: {tool_name}")
}

// ---------------------------------------------------------------------------
// Compatibility shims for `main.rs` (kept stable while modules were split).
// ---------------------------------------------------------------------------

/// All tool definitions, aggregated from every submodule.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    all_tool_definitions()
}

/// Dispatch a tool call; errors propagate to the caller.
pub async fn handle_tool_call(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<CallToolResult> {
    dispatch_tool_call(client, tool_name, args).await
}
