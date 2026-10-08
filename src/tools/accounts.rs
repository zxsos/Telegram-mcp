//! Accounts tools.
//!
//! Single-account build: `list_accounts` returns the profile of the
//! currently authorized account.

use crate::mcp::{CallToolResult, ToolDefinition};
use grammers_client::Client;
use grammers_tl_types as tl;
use serde_json::Value;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "list_accounts".into(),
        description: "List the configured Telegram account with profile info (name, phone, status). Single-account build: returns the current account.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
    }]
}

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    _args: &Value,
) -> anyhow::Result<Option<CallToolResult>> {
    match tool_name {
        "list_accounts" => Ok(Some(CallToolResult::ok(list_accounts(client).await?))),
        _ => Ok(None),
    }
}

fn status_name(status: &tl::enums::UserStatus) -> &'static str {
    match status {
        tl::enums::UserStatus::Empty => "empty",
        tl::enums::UserStatus::Online(_) => "online",
        tl::enums::UserStatus::Offline(_) => "offline",
        tl::enums::UserStatus::Recently(_) => "recently",
        tl::enums::UserStatus::LastWeek(_) => "last_week",
        tl::enums::UserStatus::LastMonth(_) => "last_month",
    }
}

async fn list_accounts(client: &Client) -> anyhow::Result<String> {
    let me = client.get_me().await?;
    let name = format!(
        "{} {}",
        me.first_name().unwrap_or(""),
        me.last_name().unwrap_or("")
    );
    let name = name.trim();
    let name = if name.is_empty() { "Unknown" } else { name };
    let phone = me.phone().map_or("N/A".to_string(), |p| format!("+{p}"));
    let status = status_name(me.status());
    Ok(format!("current: {name} ({phone}) — {status}"))
}
