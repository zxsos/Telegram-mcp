//! Claude Code MCP registration helpers.
//!
//! Original from telegram-mcp (MIT, (c) 2026 septagram).

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::PathBuf;

fn claude_json_path() -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .context("Neither USERPROFILE nor HOME is set")?;
    Ok(PathBuf::from(home).join(".claude.json"))
}

fn mcp_entry() -> Value {
    serde_json::json!({
        "type": "stdio",
        "command": "telegram-mcp",
        "args": []
    })
}

pub fn register_mcp() -> Result<()> {
    let path = claude_json_path()?;

    let mut root: Value = match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => bail!("Failed to read {}: {e}", path.display()),
    };

    let servers = root
        .as_object_mut()
        .context("claude.json root is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    let servers = servers
        .as_object_mut()
        .context("mcpServers is not an object")?;

    servers.insert("telegram-mcp".into(), mcp_entry());

    let output = serde_json::to_string_pretty(&root)?;
    std::fs::write(&path, output).with_context(|| format!("Failed to write {}", path.display()))?;

    eprintln!("Registered telegram-mcp MCP server in {}", path.display());
    Ok(())
}

pub fn unregister_mcp() -> Result<()> {
    let path = claude_json_path()?;

    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("No {} found, nothing to unregister", path.display());
            return Ok(());
        }
        Err(e) => bail!("Failed to read {}: {e}", path.display()),
    };

    let mut root: Value = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    let removed = root
        .as_object_mut()
        .and_then(|obj| obj.get_mut("mcpServers"))
        .and_then(|servers| servers.as_object_mut())
        .and_then(|servers| servers.remove("telegram-mcp"))
        .is_some();

    if removed {
        let output = serde_json::to_string_pretty(&root)?;
        std::fs::write(&path, output)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        eprintln!(
            "Unregistered telegram-mcp MCP server from {}",
            path.display()
        );
    } else {
        eprintln!(
            "telegram-mcp not found in {}, nothing to remove",
            path.display()
        );
    }

    Ok(())
}
