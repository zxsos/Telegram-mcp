//! Event-driven incoming-message tools (wait / debounce / feed).
//!
//! TODO: not implemented yet.
//!
//! grammers 0.10 *can* stream updates, but only through the
//! `mpsc::UnboundedReceiver<UpdatesLike>` produced by `SenderPool::new` at
//! connect time (`client.stream_updates(receiver, config)`). That receiver is
//! currently discarded in `src/auth.rs`, and these tools need server-level
//! state that a stateless per-call `&Client` cannot hold:
//!
//! - a persistent consumer task feeding a shared pending-message map
//!   (incoming private, non-bot, non-self messages per chat),
//! - a debounce ("settle window") scanner shared across calls,
//! - the JSONL incoming feed file + writer task for callback mode.
//!
//! Wiring this up requires keeping the updates receiver in `src/auth.rs`,
//! spawning one consumer task per client at startup, and sharing the pending
//! map (e.g. `Arc<Mutex<...>>`) with the tool handlers. Until then every
//! tool below returns `not supported: ...` instead of silently misbehaving.

use crate::mcp::{CallToolResult, ToolDefinition};
use grammers_client::Client;
use serde_json::{Value, json};

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "wait_for_new_message".into(),
            description: "Block until a new incoming private message from a non-bot user arrives, then return the chats with pending (unprocessed) messages. Returns {\"event\": false, \"reason\": \"timeout\"} on timeout. Does NOT consume the pending set.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "timeout": {"type": "number", "description": "Max seconds to block (default 50)", "default": 50},
                    "chat_id": {"description": "Wait for THIS chat only (bare id or @username). Without it any unrelated conversation wakes the call."}
                }
            }),
        },
        ToolDefinition {
            name: "wait_for_settled_message".into(),
            description: "Event-driven DEBOUNCED wait. Blocks until some private user chat received incoming messages AND then went quiet for settle_ms, delivering the burst as ONE settled event and removing it from the pending set. Returns {\"event\": false, \"reason\": \"timeout\"} on timeout.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "settle_ms": {"type": "integer", "description": "Quiet period after the last message before a burst is settled (default 6000)", "default": 6000},
                    "max_wait_ms": {"type": "integer", "description": "Max total time to block in ms (default 50000)", "default": 50000},
                    "chat_id": {"description": "Wait for THIS chat only (bare id or @username)."}
                }
            }),
        },
        ToolDefinition {
            name: "enable_incoming_feed".into(),
            description: "Enable callback mode: a background task appends every settled incoming burst as one JSON line to the feed file, so an external watcher can wake the agent per event instead of blocking in wait_for_settled_message. While enabled it consumes settled bursts, so don't mix with wait_for_settled_message.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "settle_ms": {"type": "integer", "description": "Quiet period before a burst is written (default 6000)", "default": 6000}
                }
            }),
        },
        ToolDefinition {
            name: "disable_incoming_feed".into(),
            description: "Disable the incoming event feed (stops writing to the feed file).".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "incoming_feed_status".into(),
            description: "Report whether the incoming event feed is enabled, its file path, and the watch command for waking an agent per event.".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
    ]
}

pub async fn try_handle(
    _client: &Client,
    tool_name: &str,
    _args: &Value,
) -> anyhow::Result<Option<CallToolResult>> {
    // TODO: implement once the update stream is wired at the server level
    // (see module docs). grammers 0.10 needs the SenderPool updates receiver,
    // which is not available from a stateless per-call &Client.
    match tool_name {
        "wait_for_new_message"
        | "wait_for_settled_message"
        | "enable_incoming_feed"
        | "disable_incoming_feed"
        | "incoming_feed_status" => {
            anyhow::bail!("not supported: event-driven tools need the Telegram update stream, which is not wired in this build (see TODO in tools/events.rs)")
        }
        _ => Ok(None),
    }
}
