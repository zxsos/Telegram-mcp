//! Legacy compatibility tools (original 6-tool API).

use anyhow::{Context, Result, bail};
use grammers_client::Client;
use grammers_client::peer::Peer;
use grammers_session::types::PeerRef;
use serde_json::Value;

use crate::mcp::{CallToolResult, ToolDefinition};
use crate::telegram::resolve;

use super::{str_arg, usize_arg};

/// Resolve chat by numeric ID or @username (handles both).
async fn resolve_chat_legacy(client: &Client, chat_id: &str) -> Result<PeerRef> {
    // Try numeric ID first
    if let Ok(id) = chat_id.parse::<i64>() {
        let mut dialogs = client.iter_dialogs();
        while let Some(d) = dialogs.next().await? {
            if d.peer().id().bare_id() == id {
                return Ok(d.peer_ref());
            }
        }
        bail!("No chat found with id {id}. Use list_chats to find IDs.");
    }
    // Fall back to username resolution
    resolve(client, chat_id).await
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "search_in_channel".into(),
            description: "Search keywords within a specific channel or group".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": {"type": "string", "description": "Numeric chat ID or @username"},
                    "query": {"type": "string", "description": "Search keywords"},
                    "limit": {"type": "integer", "description": "Max results (default 20)"}
                },
                "required": ["chat_id", "query"]
            }),
        },
        ToolDefinition {
            name: "get_messages_context".into(),
            description: "Get context messages around a specific message".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": {"type": "string", "description": "Numeric chat ID or @username"},
                    "message_id": {"type": "integer", "description": "Message ID"},
                    "before": {"type": "integer", "description": "Messages before (default 5)"},
                    "after": {"type": "integer", "description": "Messages after (default 5)"}
                },
                "required": ["chat_id", "message_id"]
            }),
        },
        ToolDefinition {
            name: "get_profile".into(),
            description: "Get user, group or channel profile info".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "username": {"type": "string", "description": "@username or user ID"}
                },
                "required": ["username"]
            }),
        },
        ToolDefinition {
            name: "join_channel".into(),
            description: "Join a public channel by @username".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string", "description": "@username of the channel"}
                },
                "required": ["channel"]
            }),
        },
    ]
}

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> Result<Option<CallToolResult>> {
    let result = match tool_name {
        "search_in_channel" => {
            let chat_id = str_arg(args, "chat_id");
            let query = str_arg(args, "query");
            let limit = usize_arg(args, "limit", 20);
            search_in_channel(client, chat_id, query, limit, 2).await?
        }
        "get_messages_context" => {
            let chat_id = str_arg(args, "chat_id");
            let message_id = args.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let before = usize_arg(args, "before", 5);
            let after = usize_arg(args, "after", 5);
            get_messages_context(client, chat_id, message_id, before, after).await?
        }
        "get_profile" => {
            let username = str_arg(args, "username");
            get_profile(client, username).await?
        }
        "join_channel" => {
            let channel = str_arg(args, "channel");
            join_channel(client, channel).await?
        }
        _ => return Ok(None),
    };
    Ok(Some(CallToolResult::ok(result)))
}

async fn search_in_channel(
    client: &Client,
    channel: &str,
    query: &str,
    limit: usize,
    context_count: usize,
) -> Result<String> {
    let peer = resolve_chat_legacy(client, channel).await?;

    let mut search = client.search_messages(peer).query(query);
    let mut results = Vec::new();

    for _ in 0..limit {
        match search.next().await {
            Ok(Some(msg)) => results.push(msg),
            Ok(None) => break,
            Err(e) => bail!("Search error: {e}"),
        }
    }

    if results.is_empty() {
        return Ok(format!("No messages found for '{query}'."));
    }

    let mut out = String::new();
    for msg in &results {
        if context_count > 0 {
            let msg_id = msg.id();
            let before_ids: Vec<i32> = ((msg_id - context_count as i32)..msg_id).collect();
            let after_ids: Vec<i32> = ((msg_id + 1)..=(msg_id + context_count as i32)).collect();
            let mut all_ids = before_ids;
            all_ids.push(msg_id);
            all_ids.extend(after_ids);

            match client.get_messages_by_id(peer, &all_ids).await {
                Ok(context_msgs) => {
                    out.push_str(&format!("--- Match (msg {msg_id}) with context ---\n"));
                    for ctx_msg in context_msgs.into_iter().flatten() {
                        format_message(&mut out, &ctx_msg, ctx_msg.id() == msg_id);
                    }
                    out.push('\n');
                }
                Err(_) => {
                    out.push_str(&format!("--- Match (msg {msg_id}) ---\n"));
                    format_message(&mut out, msg, true);
                    out.push('\n');
                }
            }
        } else {
            format_message(&mut out, msg, true);
        }
    }
    Ok(out)
}

async fn get_messages_context(
    client: &Client,
    channel: &str,
    message_id: i32,
    before: usize,
    after: usize,
) -> Result<String> {
    let peer = resolve_chat_legacy(client, channel).await?;
    let start = message_id - before as i32;
    let end = message_id + after as i32;
    let ids: Vec<i32> = (start..=end).collect();

    let messages = client
        .get_messages_by_id(peer, &ids)
        .await
        .context("Failed to get messages")?;

    let mut out = String::new();
    for msg in messages.into_iter().flatten() {
        format_message(&mut out, &msg, msg.id() == message_id);
    }
    if out.is_empty() {
        out = "No messages found at this ID.".into();
    }
    Ok(out)
}

async fn get_profile(client: &Client, username: &str) -> Result<String> {
    let username = username.strip_prefix('@').unwrap_or(username);
    let peer = client
        .resolve_username(username)
        .await
        .context("Failed to resolve username")?
        .with_context(|| format!("No user/channel found for @{username}"))?;

    let mut out = String::new();
    let name = peer.name().unwrap_or("(no name)");
    let uname = peer.username().map_or(String::new(), |u| format!("@{u}"));

    match &peer {
        Peer::User(user) => {
            let last = user.last_name().unwrap_or("");
            let bot = if user.is_bot() { " [BOT]" } else { "" };
            out.push_str(&format!("User: {name} {last}{bot}\n"));
            out.push_str(&format!("Username: {uname}\n"));
            out.push_str(&format!("ID: {}\n", user.id().bare_id()));
        }
        Peer::Group(group) => {
            let title = group.title().unwrap_or(name);
            out.push_str(&format!("Group: {title}\n"));
            out.push_str(&format!("Username: {uname}\n"));
            out.push_str(&format!("ID: {}\n", group.id().bare_id()));
        }
        Peer::Channel(channel) => {
            let title = channel.title();
            out.push_str(&format!("Channel: {title}\n"));
            out.push_str(&format!("Username: {uname}\n"));
            out.push_str(&format!("ID: {}\n", channel.id().bare_id()));
        }
    }
    Ok(out)
}

async fn join_channel(client: &Client, channel: &str) -> Result<String> {
    let peer = resolve_chat_legacy(client, channel).await?;
    client
        .join_chat(peer)
        .await
        .context("Failed to join channel")?;
    Ok(format!("Joined {channel} successfully."))
}

fn format_message(out: &mut String, msg: &grammers_client::message::Message, is_match: bool) {
    let marker = if is_match { ">>>" } else { "   " };
    let date = msg.date().format("%Y-%m-%d %H:%M");
    let sender_name = msg
        .sender()
        .and_then(|p| p.name().map(String::from))
        .unwrap_or_else(|| "?".into());
    let text = msg.text();
    let truncated = if text.len() > 500 {
        let end = text.floor_char_boundary(500);
        format!("{}…", &text[..end])
    } else {
        text.to_string()
    };
    out.push_str(&format!(
        "{marker} [{date}] {sender_name} (id:{id}): {truncated}\n",
        id = msg.id()
    ));
}
