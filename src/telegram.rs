//! Telegram API operations.
//!
//! Original from telegram-mcp (MIT, (c) 2026 septagram).
//! Added: `list_chats`, `get_messages` for monitoring private groups.
//! Updated for grammers 0.10 API (iter_dialogs, no search_peer).

use anyhow::{Context, Result, bail};
use grammers_client::Client;
use grammers_client::peer::Peer;
use grammers_session::types::PeerRef;

/// Resolve a "@username" or "username" to a PeerRef.
pub async fn resolve(client: &Client, channel: &str) -> Result<PeerRef> {
    let username = channel.strip_prefix('@').unwrap_or(channel);
    let peer = client
        .resolve_username(username)
        .await
        .context("Failed to resolve username")?
        .with_context(|| format!("No channel/user found for @{username}"))?;
    Ok(peer
        .to_ref()
        .await
        .context("Failed to get peer ref")?)
}

/// Get the type string for a Peer.
fn peer_kind(peer: &Peer) -> &'static str {
    match peer {
        Peer::User(_) => "dm",
        Peer::Group(_) => "group",
        Peer::Channel(_) => "channel",
    }
}

/// List all dialogs (chats, groups, channels) the user is in.
/// Returns id, name, type — used to find private groups to monitor.
pub async fn list_chats(client: &Client, limit: usize) -> Result<String> {
    let mut dialogs = client.iter_dialogs();
    let mut out = String::new();
    let mut count = 0;

    while let Some(dialog) = dialogs.next().await.context("Failed to fetch dialog")? {
        if count >= limit {
            break;
        }
        let peer = dialog.peer();
        let kind = peer_kind(peer);
        let name = peer.name().unwrap_or("(no name)");
        // PeerId to i64 for display
        let id = peer.id().bare_id();
        out.push_str(&format!("- id:{id} | {kind} | {name}\n"));
        count += 1;
    }

    if out.is_empty() {
        out = "No chats found.".into();
    }

    Ok(out)
}

/// Get recent messages from a chat by its numeric ID.
/// Works for private groups (no username needed) — core for monitoring.
pub async fn get_messages(client: &Client, chat_id: i64, limit: usize) -> Result<String> {
    let mut dialogs = client.iter_dialogs();
    let mut target_peer: Option<PeerRef> = None;

    while let Some(dialog) = dialogs.next().await.context("Failed to fetch dialog")? {
        let peer = dialog.peer();
        if peer.id().bare_id() == chat_id {
            target_peer = Some(dialog.peer_ref());
            break;
        }
    }

    let peer = target_peer
        .with_context(|| format!("No chat found with id {chat_id}. Use list_chats to find IDs."))?;

    let mut out = String::new();
    let mut iter = client.iter_messages(peer);
    let mut count = 0;

    while let Some(msg) = iter.next().await.context("Failed to fetch message")? {
        format_message(&mut out, &msg, false);
        count += 1;
        if count >= limit {
            break;
        }
    }

    if out.is_empty() {
        out = "No messages found in this chat.".into();
    }

    Ok(out)
}

/// Search messages in a specific channel with optional context.
pub async fn search_in_channel(
    client: &Client,
    channel: &str,
    query: &str,
    limit: usize,
    context_count: usize,
) -> Result<String> {
    let peer = resolve(client, channel).await?;

    let mut search = client.search_messages(peer.clone()).query(query);
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
        // Fetch context messages around the hit
        if context_count > 0 {
            let msg_id = msg.id();
            let before_ids: Vec<i32> = ((msg_id - context_count as i32)..msg_id).collect();
            let after_ids: Vec<i32> = ((msg_id + 1)..=(msg_id + context_count as i32)).collect();

            let mut all_ids = before_ids;
            all_ids.push(msg_id);
            all_ids.extend(after_ids);

            match client.get_messages_by_id(peer.clone(), &all_ids).await {
                Ok(context_msgs) => {
                    out.push_str(&format!("--- Match (msg {msg_id}) with context ---\n"));
                    for ctx_msg in context_msgs.into_iter().flatten() {
                        format_message(&mut out, &ctx_msg, ctx_msg.id() == msg_id);
                    }
                    out.push('\n');
                }
                Err(_) => {
                    // Fallback: just show the match
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

/// Get messages around a specific message ID.
pub async fn get_messages_context(
    client: &Client,
    channel: &str,
    message_id: i32,
    before: usize,
    after: usize,
) -> Result<String> {
    let peer = resolve(client, channel).await?;

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

/// Get profile info for a user or channel.
pub async fn get_profile(client: &Client, username: &str) -> Result<String> {
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

/// Join a channel.
pub async fn join_channel(client: &Client, channel: &str) -> Result<String> {
    let peer = resolve(client, channel).await?;

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
