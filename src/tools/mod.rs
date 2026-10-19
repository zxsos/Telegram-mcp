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
pub mod legacy;
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
    defs.extend(legacy::tool_definitions());
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
    if let Some(result) = legacy::try_handle(client, tool_name, args).await? {
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

#[cfg(test)]
mod tests {
    //! Tool surface quality tests.
    //!
    //! Verifies the MCP tool definitions meet quality standards:
    //! - Every tool declares all four MCP annotations (readOnlyHint,
    //!   destructiveHint, idempotentHint, openWorldHint)
    //! - Tool names are unique
    //! - Input schemas are valid

    use super::all_tool_definitions;
    use std::collections::HashSet;

    /// Every tool must declare all four MCP annotations.
    #[test]
    fn all_tools_have_complete_annotations() {
        let defs = all_tool_definitions();
        assert!(!defs.is_empty(), "no tools defined");

        let missing: Vec<_> = defs
            .iter()
            .filter(|d| d.annotations.is_none())
            .map(|d| d.name.clone())
            .collect();

        assert!(
            missing.is_empty(),
            "tools missing annotations: {}",
            missing.join(", ")
        );
    }

    /// Annotations must serialize to the exact MCP field names as booleans.
    #[test]
    fn annotations_serialize_to_mcp_field_names() {
        let defs = all_tool_definitions();

        for def in &defs {
            let ann = def
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("tool {} missing annotations", def.name));

            let json = serde_json::to_value(ann).expect("annotations must serialize");
            for field in [
                "readOnlyHint",
                "destructiveHint",
                "idempotentHint",
                "openWorldHint",
            ] {
                assert!(
                    json.get(field).and_then(|v| v.as_bool()).is_some(),
                    "tool {} annotation field {} must be an explicit boolean",
                    def.name,
                    field
                );
            }
        }
    }

    /// Tool names must be unique across the whole surface.
    #[test]
    fn tool_names_are_unique() {
        let defs = all_tool_definitions();
        let mut seen = HashSet::new();
        let mut dupes = Vec::new();

        for def in &defs {
            if !seen.insert(def.name.clone()) {
                dupes.push(def.name.clone());
            }
        }

        assert!(
            dupes.is_empty(),
            "duplicate tool names: {}",
            dupes.join(", ")
        );
    }

    /// Every tool must have a non-empty description and a valid input schema.
    #[test]
    fn tools_have_description_and_valid_schema() {
        let defs = all_tool_definitions();

        for def in &defs {
            assert!(
                !def.description.trim().is_empty(),
                "tool {} has empty description",
                def.name
            );
            assert!(
                def.input_schema.is_object(),
                "tool {} inputSchema must be a JSON object",
                def.name
            );
            assert_eq!(
                def.input_schema.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "tool {} inputSchema.type must be \"object\"",
                def.name
            );
        }
    }

    /// Annotation consistency: destructive tools marked destructive,
    /// read-only tools never destructive.
    #[test]
    fn annotation_consistency() {
        let defs = all_tool_definitions();

        for def in &defs {
            let ann = def.annotations.as_ref().unwrap();
            let name = def.name.as_str();

            if name.starts_with("delete_") || name == "ban_user" {
                assert!(
                    ann.destructive_hint,
                    "tool {name} deletes data but destructiveHint is false"
                );
                assert!(
                    !ann.read_only_hint,
                    "tool {name} deletes data but readOnlyHint is true"
                );
            }

            if ann.read_only_hint {
                assert!(
                    !ann.destructive_hint,
                    "tool {name} is read-only but destructiveHint is true"
                );
            }
        }
    }
}
