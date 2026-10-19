//! MCP (Model Context Protocol) JSON-RPC 2.0 protocol types.
//!
//! Original from telegram-mcp (MIT, (c) 2026 septagram).

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ── MCP Protocol Types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: ToolsCapability,
}

#[derive(Debug, Serialize)]
pub struct ToolsCapability {}

#[derive(Debug, Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnlyHint")]
    pub read_only_hint: bool,
    #[serde(rename = "destructiveHint")]
    pub destructive_hint: bool,
    #[serde(rename = "idempotentHint")]
    pub idempotent_hint: bool,
    #[serde(rename = "openWorldHint")]
    pub open_world_hint: bool,
}

#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// MCP tool annotations for a tool, keyed by tool name.
///
/// Tuple is `(read_only_hint, destructive_hint, idempotent_hint)`;
/// `open_world_hint` is always `true` because every tool talks to
/// Telegram's servers (the external world).
pub fn annotations_for(tool_name: &str) -> ToolAnnotations {
    let (read_only, destructive, idempotent): (bool, bool, bool) = match tool_name {
        // ---- accounts ----
        "list_accounts" => (true, false, true),
        // ---- chats ----
        "get_chats" => (true, false, true),
        "subscribe_public_channel" => (false, false, true),
        "list_topics" => (true, false, true),
        "enable_forum_topics" => (false, false, true),
        "create_forum_topic" => (false, false, false),
        "edit_forum_topic" => (false, false, true),
        "delete_forum_topic" => (false, true, true),
        "list_chats" => (true, false, true),
        "get_chat" => (true, false, true),
        "search_public_chats" => (true, false, true),
        "resolve_username" => (true, false, true),
        "get_full_chat" => (true, false, true),
        "mute_chat" => (false, false, true),
        "unmute_chat" => (false, false, true),
        "archive_chat" => (false, false, true),
        "unarchive_chat" => (false, false, true),
        "get_common_chats" => (true, false, true),
        "get_message_read_by" => (true, false, true),
        "get_message_link" => (true, false, true),
        // ---- contacts ----
        "list_contacts" => (true, false, true),
        "search_contacts" => (true, false, true),
        "get_contact_ids" => (true, false, true),
        "get_direct_chat_by_contact" => (true, false, true),
        "get_contact_chats" => (true, false, true),
        "get_last_interaction" => (true, false, true),
        "add_contact" => (false, false, true),
        "delete_contact" => (false, true, true),
        "block_user" => (false, false, true),
        "unblock_user" => (false, false, true),
        "import_contacts" => (false, false, true),
        "export_contacts" => (true, false, true),
        "get_blocked_users" => (true, false, true),
        "send_contact" => (false, false, false),
        "set_contact_alias" => (false, false, true),
        "list_contact_aliases" => (true, false, true),
        "delete_contact_alias" => (false, true, true),
        // ---- events ----
        "wait_for_new_message" => (true, false, true),
        "wait_for_settled_message" => (true, false, true),
        "enable_incoming_feed" => (false, false, true),
        "disable_incoming_feed" => (false, false, true),
        "incoming_feed_status" => (true, false, true),
        // ---- folders ----
        "get_folder_limits" => (true, false, true),
        "get_folder_snapshot" => (true, false, true),
        "update_folder" => (false, false, true),
        "list_folders" => (true, false, true),
        "get_folder" => (true, false, true),
        "create_folder" => (false, false, false),
        "add_chat_to_folder" => (false, false, true),
        "remove_chat_from_folder" => (false, false, true),
        "delete_folder" => (false, true, true),
        "reorder_folders" => (false, false, true),
        // ---- groups ----
        "create_group" => (false, false, false),
        "invite_to_group" => (false, false, true),
        "leave_chat" => (false, false, true),
        "get_participants" => (true, false, true),
        "create_channel" => (false, false, false),
        "edit_chat_title" => (false, false, true),
        "edit_chat_photo" => (false, false, true),
        "edit_chat_about" => (false, false, true),
        "delete_chat_photo" => (false, true, true),
        "promote_admin" => (false, false, true),
        "demote_admin" => (false, false, true),
        "ban_user" => (false, true, true),
        "unban_user" => (false, false, true),
        "remove_user" => (false, true, true),
        "set_default_chat_permissions" => (false, false, true),
        "toggle_slow_mode" => (false, false, true),
        "edit_admin_rights" => (false, false, true),
        "get_admins" => (true, false, true),
        "get_member_admin_status" => (true, false, true),
        "get_banned_users" => (true, false, true),
        "get_invite_link" => (true, false, true),
        "join_chat_by_link" => (false, false, true),
        "export_chat_invite" => (false, false, false),
        "import_chat_invite" => (false, false, true),
        "get_recent_actions" => (true, false, true),
        // ---- legacy ----
        "search_in_channel" => (true, false, true),
        "get_messages_context" => (true, false, true),
        "get_profile" => (true, false, true),
        "join_channel" => (false, false, true),
        // ---- media ----
        "send_file" => (false, false, false),
        "send_album" => (false, false, false),
        "download_media" => (true, false, true),
        "send_voice" => (false, false, false),
        "upload_file" => (false, false, false),
        "get_media_info" => (true, false, true),
        "get_sticker_sets" => (true, false, true),
        "send_sticker" => (false, false, false),
        "get_gif_search" => (true, false, true),
        "send_gif" => (false, false, false),
        "list_photos" => (true, false, true),
        "open_photo" => (true, false, true),
        "get_photo_sheet" => (true, false, true),
        "inspect_document" => (true, false, true),
        // ---- messages ----
        "get_messages" => (true, false, true),
        "send_message" => (false, false, false),
        "send_scheduled_message" => (false, false, false),
        "get_scheduled_messages" => (true, false, true),
        "delete_scheduled_message" => (false, true, true),
        "list_inline_buttons" => (true, false, true),
        "press_inline_button" => (false, false, false),
        "list_messages" => (true, false, true),
        "transcribe_voice" => (true, false, true),
        "get_message_context" => (true, false, true),
        "get_send_as" => (true, false, true),
        "forward_message" => (false, false, false),
        "forward_messages" => (false, false, false),
        "edit_message" => (false, false, true),
        "delete_message" => (false, true, true),
        "delete_chat_history" => (false, true, true),
        "delete_messages_bulk" => (false, true, true),
        "pin_message" => (false, false, true),
        // ---- profile ----
        "get_me" => (true, false, true),
        "update_profile" => (false, false, true),
        "set_profile_photo" => (false, false, true),
        "delete_profile_photo" => (false, true, true),
        "get_privacy_settings" => (true, false, true),
        "set_privacy_settings" => (false, false, true),
        "get_full_user" => (true, false, true),
        "get_bot_info" => (true, false, true),
        "set_bot_commands" => (false, false, true),
        "get_user_photos" => (true, false, true),
        "get_user_status" => (true, false, true),
        // Conservative default: not read-only, not idempotent.
        _ => (false, false, false),
    };
    ToolAnnotations {
        read_only_hint: read_only,
        destructive_hint: destructive,
        idempotent_hint: idempotent,
        open_world_hint: true,
    }
}

#[derive(Debug, Serialize)]
pub struct ToolsListResult {
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ContentItem {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Serialize)]
pub struct CallToolResult {
    pub content: Vec<ContentItem>,
    #[serde(rename = "isError", skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl CallToolResult {
    pub fn ok(text: String) -> Self {
        Self {
            content: vec![ContentItem::Text { text }],
            is_error: false,
        }
    }

    pub fn err(text: String) -> Self {
        Self {
            content: vec![ContentItem::Text { text }],
            is_error: true,
        }
    }
}
