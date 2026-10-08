//! MCP tool definitions and dispatch.
//!
//! Original from telegram-mcp (MIT, (c) 2026 septagram).
//! Added: `list_chats`, `get_messages` for private group monitoring.
//! Removed: `search_channels` (search_peer not in grammers 0.10 API).

use crate::mcp::{CallToolResult, ToolDefinition};
use crate::telegram;
use anyhow::bail;
use grammers_client::Client;
use serde_json::Value;

/// Get a string argument, defaulting to "".
fn str_arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// Get a usize argument with default.
fn usize_arg(args: &Value, key: &str, default: u64) -> usize {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default) as usize
}

/// Get an i64 argument with default.
fn i64_arg(args: &Value, key: &str, default: i64) -> i64 {
    args.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_chats".into(),
            description: "List all chats (groups, channels, DMs) the user is in, with IDs. Use to find a private group's ID for monitoring.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Max number of chats (default: 50)",
                        "default": 50
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_messages".into(),
            description: "Get recent messages from a chat by its numeric ID. Works for private groups (no username needed). Core tool for monitoring.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": {
                        "type": "integer",
                        "description": "Numeric chat ID (from list_chats)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max number of messages, newest first (default: 20)",
                        "default": 20
                    }
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "search_in_channel".into(),
            description: "Search messages within a specific Telegram channel by username.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel username (e.g. '@example' or 'example')"
                    },
                    "query": {
                        "type": "string",
                        "description": "Text to search for"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max number of results (default: 10)",
                        "default": 10
                    },
                    "context": {
                        "type": "integer",
                        "description": "Number of messages to show before and after each match (default: 2)",
                        "default": 2
                    }
                },
                "required": ["channel", "query"]
            }),
        },
        ToolDefinition {
            name: "get_messages_context".into(),
            description: "Get messages around a specific message ID in a channel. Useful for reading conversation context.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel username (e.g. '@example' or 'example')"
                    },
                    "message_id": {
                        "type": "integer",
                        "description": "Message ID to center on"
                    },
                    "before": {
                        "type": "integer",
                        "description": "Number of messages before (default: 5)",
                        "default": 5
                    },
                    "after": {
                        "type": "integer",
                        "description": "Number of messages after (default: 5)",
                        "default": 5
                    }
                },
                "required": ["channel", "message_id"]
            }),
        },
        ToolDefinition {
            name: "get_profile".into(),
            description: "Get profile information for a Telegram user or channel.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "username": {
                        "type": "string",
                        "description": "Username to look up (e.g. '@example' or 'example')"
                    }
                },
                "required": ["username"]
            }),
        },
        ToolDefinition {
            name: "join_channel".into(),
            description: "Join a public Telegram channel by username.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel username (e.g. '@example' or 'example')"
                    }
                },
                "required": ["channel"]
            }),
        },
    ]
}

pub async fn handle_tool_call(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<CallToolResult> {
    let text = match tool_name {
        "list_chats" => {
            let limit = usize_arg(args, "limit", 50);
            telegram::list_chats(client, limit).await?
        }
        "get_messages" => {
            let chat_id = i64_arg(args, "chat_id", 0);
            let limit = usize_arg(args, "limit", 20);
            telegram::get_messages(client, chat_id, limit).await?
        }
        "search_in_channel" => {
            let channel = str_arg(args, "channel");
            let query = str_arg(args, "query");
            let limit = usize_arg(args, "limit", 10);
            let context = usize_arg(args, "context", 2);
            telegram::search_in_channel(client, channel, query, limit, context).await?
        }
        "get_messages_context" => {
            let channel = str_arg(args, "channel");
            let message_id = i64_arg(args, "message_id", 0) as i32;
            let before = usize_arg(args, "before", 5);
            let after = usize_arg(args, "after", 5);
            telegram::get_messages_context(client, channel, message_id, before, after).await?
        }
        "get_profile" => {
            let username = str_arg(args, "username");
            telegram::get_profile(client, username).await?
        }
        "join_channel" => {
            let channel = str_arg(args, "channel");
            telegram::join_channel(client, channel).await?
        }
        _ => bail!("Unknown tool: {tool_name}"),
    };

    Ok(CallToolResult::ok(text))
}
