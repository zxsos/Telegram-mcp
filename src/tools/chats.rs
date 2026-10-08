//! Chat management tools.
//!
//! Ported from `telegram_mcp/tools/chats.py` (Telethon) to grammers 0.10:
//! dialog listing, forum topics, mute/archive, read receipts, message links.

use super::{bool_arg, i64_arg, str_arg, usize_arg};
use crate::mcp::{CallToolResult, ToolDefinition};
use anyhow::{Context, Result, bail};
use grammers_client::peer::{Dialog, Peer};
use grammers_client::{Client, InvocationError, tl};
use grammers_session::types::{PeerKind, PeerRef};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Chat/user IDs may arrive as JSON strings ("@name", "12345") or numbers.
fn id_arg(args: &Value, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn opt_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| v.as_i64())
}

fn opt_i32(args: &Value, key: &str) -> Option<i32> {
    opt_i64(args, key).map(|v| v as i32)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

/// Resolve "@username" / "username" / "12345" to a full `Peer`.
async fn resolve_peer(client: &Client, chat_id: &str) -> Result<Peer> {
    let chat_id = chat_id.trim();
    if chat_id.is_empty() {
        bail!("Missing chat identifier.");
    }
    if let Ok(id) = chat_id.parse::<i64>() {
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await.map_err(|e| anyhow::anyhow!(e))? {
            if dialog.peer_id().bare_id() == id {
                return Ok(dialog.peer().clone());
            }
        }
        bail!("No chat found with id {id}. Use list_chats to find IDs.");
    }
    let username = chat_id.strip_prefix('@').unwrap_or(chat_id);
    client.resolve_username(username)
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .with_context(|| format!("No user/chat found for @{username}"))
}

async fn peer_ref(peer: &Peer, label: &str) -> Result<PeerRef> {
    peer.to_ref()
        .await
        .with_context(|| format!("Cannot build input peer for {label}"))
}

fn kind_of(peer: &Peer) -> &'static str {
    match peer {
        Peer::User(_) => "user",
        Peer::Group(_) => "group",
        Peer::Channel(_) => "channel",
    }
}

fn peer_name(peer: &Peer) -> String {
    peer.name().unwrap_or("(no name)").to_string()
}

fn bare_id(peer: &Peer) -> i64 {
    peer.id().bare_id()
}

/// The peer must be a supergroup (megagroup). Used by forum-topic tools.
fn require_megagroup(peer: &Peer) -> Result<()> {
    match peer {
        Peer::Group(g) => match &g.raw {
            tl::enums::Chat::Channel(c) if c.megagroup => Ok(()),
            _ => bail!("The specified chat is not a supergroup."),
        },
        _ => bail!("The specified chat is not a supergroup."),
    }
}

/// The raw `Channel` for a forum-enabled supergroup, or a friendly error.
fn forum_channel(peer: &Peer) -> Result<()> {
    require_megagroup(peer)?;
    match peer {
        Peer::Group(g) => match &g.raw {
            tl::enums::Chat::Channel(c) if c.forum => Ok(()),
            _ => bail!(
                "This supergroup does not have forum topics enabled. \
                 Use enable_forum_topics first."
            ),
        },
        _ => unreachable!(),
    }
}

fn chat_summary(chat: &tl::enums::Chat) -> Value {
    match chat {
        tl::enums::Chat::Channel(c) => json!({
            "chat_id": c.id,
            "title": c.title,
            "username": c.username,
            "type": if c.megagroup { "group" } else { "channel" },
        }),
        tl::enums::Chat::Chat(c) => json!({
            "chat_id": c.id,
            "title": c.title,
            "type": "group",
        }),
        other => json!({ "chat_id": other.id(), "type": "unknown" }),
    }
}

fn user_summary(user: &tl::enums::User) -> Value {
    match user {
        tl::enums::User::User(u) => {
            let name = format!(
                "{} {}",
                u.first_name.clone().unwrap_or_default(),
                u.last_name.clone().unwrap_or_default()
            );
            json!({
                "user_id": u.id,
                "name": name.trim(),
                "username": u.username,
                "bot": u.bot,
            })
        }
        _ => json!({ "user_id": null }),
    }
}

/// Unread / mute / archive metadata from a dialog's raw TL object.
fn dialog_meta(dialog: &Dialog) -> (i32, bool, bool, bool) {
    let tl::enums::Dialog::Dialog(d) = &dialog.raw else {
        return (0, false, false, false);
    };
    let muted = match &d.notify_settings {
        tl::enums::PeerNotifySettings::Settings(s) => {
            s.mute_until.map_or(false, |t| t as i64 > now_unix())
        }
    };
    (d.unread_count, d.unread_mark, muted, d.folder_id == Some(1))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = s.floor_char_boundary(max);
    format!("{}…", &s[..end])
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

async fn h_get_chats(client: &Client, args: &Value) -> Result<String> {
    let page = usize_arg(args, "page", 1).max(1);
    let page_size = usize_arg(args, "page_size", 20).clamp(1, 100);

    let mut chats = Vec::new();
    let mut dialogs = client.iter_dialogs();
    while let Some(d) = dialogs.next().await.map_err(|e| anyhow::anyhow!(e))? {
        let peer = d.peer();
        chats.push(json!({
            "chat_id": bare_id(peer),
            "title": peer_name(peer),
            "type": kind_of(peer),
        }));
    }

    let start = (page - 1) * page_size;
    if start >= chats.len() {
        return Ok("Page out of range.".into());
    }
    let end = (start + page_size).min(chats.len());
    Ok(json!({
        "page": page,
        "page_size": page_size,
        "total": chats.len(),
        "chats": &chats[start..end],
    })
    .to_string())
}

async fn h_subscribe_public_channel(client: &Client, args: &Value) -> Result<String> {
    let channel = id_arg(args, "channel");
    let peer = resolve_peer(client, &channel).await?;
    let pref = peer_ref(&peer, &channel).await?;
    if pref.id.kind() != PeerKind::Channel {
        bail!("Can only subscribe to channels/supergroups, not this peer type.");
    }
    let name = peer_name(&peer);
    let join_result = client.invoke(&tl::functions::channels::JoinChannel {
        channel: pref.into(),
    })
    .await;
    match join_result {
        Ok(_) => Ok(format!("Subscribed to {name}.")),
        Err(InvocationError::Rpc(rpc)) if rpc.name == "USER_ALREADY_PARTICIPANT" => {
            Ok(format!("Already subscribed to {name}."))
        }
        Err(e) => Err(anyhow::anyhow!(e).context(format!("Failed to subscribe to {name}"))),
    }
}

async fn h_list_topics(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let limit = i64_arg(args, "limit", 200).clamp(1, 200) as i32;
    let offset_topic = i64_arg(args, "offset_topic", 0) as i32;

    let peer = resolve_peer(client, &chat_id).await?;
    forum_channel(&peer)?;
    let pref = peer_ref(&peer, &chat_id).await?;

    let result = client.invoke(&tl::functions::messages::GetForumTopics {
        peer: pref.into(),
        q: opt_str(args, "search_query").map(str::to_string),
        offset_date: 0,
        offset_id: 0,
        offset_topic,
        limit,
    })
    .await
    .context("Failed to list forum topics")?;

    let tl::enums::messages::ForumTopics::Topics(t) = result;

    // Map top_message id -> unix timestamp for last-activity info.
    let last_activity: std::collections::HashMap<i32, i32> = t
        .messages
        .iter()
        .filter_map(|m| {
            let tl::enums::Message::Message(msg) = m else {
                return None;
            };
            Some((msg.id, msg.date))
        })
        .collect();

    let topics: Vec<Value> = t
        .topics
        .iter()
        .filter_map(|topic| {
            let tl::enums::ForumTopic::Topic(tp) = topic else {
                return None;
            };
            let mut rec = json!({
                "id": tp.id,
                "title": truncate(&tp.title, 256),
                "closed": tp.closed,
                "hidden": tp.hidden,
            });
            if tp.unread_count > 0 {
                rec["unread"] = json!(tp.unread_count);
            }
            if let Some(ts) = last_activity.get(&tp.top_message) {
                rec["last_activity_unix"] = json!(ts);
            }
            Some(rec)
        })
        .collect();

    if topics.is_empty() {
        return Ok("No topics found for this chat.".into());
    }
    Ok(json!(topics).to_string())
}

async fn h_enable_forum_topics(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let tabs = bool_arg(args, "tabs", true);

    let peer = resolve_peer(client, &chat_id).await?;
    require_megagroup(&peer)?;
    let pref = peer_ref(&peer, &chat_id).await?;
    let name = peer_name(&peer);

    client.invoke(&tl::functions::channels::ToggleForum {
        channel: pref.into(),
        enabled: true,
        tabs,
    })
    .await
    .context("Failed to enable forum topics")?;
    Ok(format!("Forum topics enabled for {name}."))
}

fn extract_topic_id(updates: &tl::enums::Updates) -> Option<i32> {
    let list = match updates {
        tl::enums::Updates::Updates(u) => &u.updates,
        tl::enums::Updates::Combined(u) => &u.updates,
        _ => return None,
    };
    for u in list {
        if let tl::enums::Update::NewChannelMessage(m) = u
            && let tl::enums::Message::Message(msg) = &m.message
        {
            return Some(msg.id);
        }
    }
    None
}

async fn h_create_forum_topic(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let title = str_arg(args, "title");
    if title.is_empty() {
        bail!("Missing required argument: title");
    }

    let peer = resolve_peer(client, &chat_id).await?;
    forum_channel(&peer)?;
    let pref = peer_ref(&peer, &chat_id).await?;

    let random_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
        & i64::MAX;

    let updates = client.invoke(&tl::functions::messages::CreateForumTopic {
        title_missing: false,
        peer: pref.into(),
        title: truncate(title, 128),
        icon_color: opt_i32(args, "icon_color"),
        icon_emoji_id: opt_i64(args, "icon_emoji_id"),
        random_id,
        send_as: None,
    })
    .await
    .context("Failed to create forum topic")?;

    let mut rec = json!({
        "chat_id": bare_id(&peer),
        "title": truncate(title, 128),
    });
    if let Some(topic_id) = extract_topic_id(&updates) {
        rec["topic_id"] = json!(topic_id);
    }
    Ok(json!([rec]).to_string())
}

async fn h_edit_forum_topic(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let topic_id = i64_arg(args, "topic_id", 0) as i32;

    let title = opt_str(args, "title").map(|s| truncate(s, 128));
    let icon_emoji_id = opt_i64(args, "icon_emoji_id");
    let closed = opt_bool(args, "closed");
    let hidden = opt_bool(args, "hidden");
    if title.is_none() && icon_emoji_id.is_none() && closed.is_none() && hidden.is_none() {
        return Ok("Nothing to change: pass title, icon_emoji_id, closed or hidden.".into());
    }

    let peer = resolve_peer(client, &chat_id).await?;
    forum_channel(&peer)?;
    let pref = peer_ref(&peer, &chat_id).await?;

    client.invoke(&tl::functions::messages::EditForumTopic {
        peer: pref.into(),
        topic_id,
        title: title.clone(),
        icon_emoji_id,
        closed,
        hidden,
    })
    .await
    .context("Failed to edit forum topic")?;

    let mut rec = json!({ "chat_id": bare_id(&peer), "topic_id": topic_id });
    if let Some(v) = title {
        rec["title"] = json!(v);
    }
    if let Some(v) = icon_emoji_id {
        rec["icon_emoji_id"] = json!(v);
    }
    if let Some(v) = closed {
        rec["closed"] = json!(v);
    }
    if let Some(v) = hidden {
        rec["hidden"] = json!(v);
    }
    Ok(json!([rec]).to_string())
}

async fn h_delete_forum_topic(client: &Client, args: &Value) -> Result<String> {
    const MAX_BATCHES: i32 = 100;
    let chat_id = id_arg(args, "chat_id");
    let topic_id = i64_arg(args, "topic_id", 0) as i32;

    let peer = resolve_peer(client, &chat_id).await?;
    forum_channel(&peer)?;
    let pref = peer_ref(&peer, &chat_id).await?;
    let input_peer: tl::enums::InputPeer = pref.into();

    // Telegram deletes topic history in batches: a non-zero offset means
    // the same request has to be sent again.
    let mut batches = 0;
    loop {
        let res = client.invoke(&tl::functions::messages::DeleteTopicHistory {
            peer: input_peer.clone(),
            top_msg_id: topic_id,
        })
        .await
        .context("Failed to delete forum topic")?;
        batches += 1;
        let tl::enums::messages::AffectedHistory::History(a) = res;
        if a.offset == 0 || batches >= MAX_BATCHES {
            break;
        }
    }

    if batches >= MAX_BATCHES {
        return Ok(format!(
            "Topic {topic_id} is still being deleted after {batches} batches; \
             call delete_forum_topic again to continue."
        ));
    }
    Ok(json!([{
        "chat_id": bare_id(&peer),
        "topic_id": topic_id,
        "deleted": true,
        "batches": batches,
    }])
    .to_string())
}

/// Unpack `messages.ChatFull` into (about, participants_count).
fn unpack_chat_full(res: tl::enums::messages::ChatFull) -> Result<(String, Option<i32>)> {
    let tl::enums::messages::ChatFull::Full(f) = res;
    match f.full_chat {
        tl::enums::ChatFull::ChannelFull(cf) => Ok((cf.about, cf.participants_count)),
        tl::enums::ChatFull::Full(gf) => {
            let count = match &gf.participants {
                tl::enums::ChatParticipants::Participants(p) => {
                    Some(p.participants.len() as i32)
                }
                tl::enums::ChatParticipants::Forbidden(_) => None,
            };
            Ok((gf.about, count))
        }
    }
}

async fn fetch_about(client: &Client, pref: &PeerRef) -> String {
    let about: Result<String> = async {
        let res = match pref.id.kind() {
            PeerKind::Channel => {
                invoke(
                    client,
                    &tl::functions::channels::GetFullChannel {
                        channel: (*pref).into(),
                    },
                )
                .await
            }
            PeerKind::Chat => {
                invoke(
                    client,
                    &tl::functions::messages::GetFullChat {
                        chat_id: pref.id.bare_id(),
                    },
                )
                .await
            }
            _ => bail!("no about for users"),
        }?;
        Ok(unpack_chat_full(res)?.0)
    }
    .await;
    truncate(&about.unwrap_or_default(), 200)
}

async fn h_list_chats(client: &Client, args: &Value) -> Result<String> {
    let chat_type = str_arg(args, "chat_type").to_lowercase();
    let limit = usize_arg(args, "limit", 20);
    let unread_only = bool_arg(args, "unread_only", false);
    let unmuted_only = bool_arg(args, "unmuted_only", false);
    let archived = opt_bool(args, "archived");
    let with_about = bool_arg(args, "with_about", false);

    let mut records = Vec::new();
    let mut scanned = 0;
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.map_err(|e| anyhow::anyhow!(e))? {
        if scanned >= limit {
            break;
        }
        scanned += 1;
        let peer = dialog.peer();
        let kind = kind_of(peer);
        if !chat_type.is_empty() && kind != chat_type {
            continue;
        }

        let (unread, unread_mark, muted, is_archived) = dialog_meta(&dialog);
        if let Some(a) = archived
            && is_archived != a
        {
            continue;
        }
        if unmuted_only && muted {
            continue;
        }
        if unread_only && unread == 0 && !unread_mark {
            continue;
        }

        let mut rec = json!({
            "chat_id": bare_id(peer),
            "title": peer_name(peer),
            "type": kind,
            "username": peer.username(),
            "unread": unread,
            "muted": muted,
            "archived": is_archived,
        });
        if unread_mark {
            rec["unread_mark"] = json!(true);
        }
        if with_about {
            let pref = peer_ref(peer, "chat").await?;
            rec["about"] = json!(fetch_about(client, &pref).await);
        }
        records.push(rec);
    }

    if records.is_empty() {
        return Ok("No chats found matching the criteria.".into());
    }
    Ok(json!(records).to_string())
}

async fn h_get_chat(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let peer = resolve_peer(client, &chat_id).await?;
    let pref = peer_ref(&peer, &chat_id).await?;

    let mut rec = json!({
        "id": pref.id.bare_id(),
        "type": kind_of(&peer),
        "username": peer.username(),
    });
    match &peer {
        Peer::User(u) => {
            rec["name"] = json!(u.full_name());
            rec["phone"] = json!(u.phone());
            rec["bot"] = json!(u.is_bot());
            rec["verified"] = json!(u.verified());
        }
        _ => {
            rec["title"] = json!(peer_name(&peer));
            let mut iter = client.iter_participants(pref);
            match iter.total().await {
                Ok(total) => rec["participants"] = json!(total),
                Err(_) => rec["participants"] = Value::Null,
            }
        }
    }

    // Dialog metadata (unread, last message) when resolved by numeric id.
    if let Ok(id) = chat_id.trim().parse::<i64>() {
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await.map_err(|e| anyhow::anyhow!(e))? {
            if dialog.peer_id().bare_id() != id {
                continue;
            }
            let (unread, unread_mark, muted, is_archived) = dialog_meta(&dialog);
            rec["unread"] = json!(unread);
            rec["muted"] = json!(muted);
            rec["archived"] = json!(is_archived);
            if unread_mark {
                rec["unread_mark"] = json!(true);
            }
            if let Some(msg) = &dialog.last_message {
                let sender = msg
                    .sender()
                    .and_then(|p| p.name().map(str::to_string))
                    .unwrap_or_else(|| "?".into());
                rec["last_message"] = json!({
                    "sender": sender,
                    "date": msg.date().format("%Y-%m-%d %H:%M").to_string(),
                    "text": truncate(&msg.text(), 500),
                });
            }
            break;
        }
    }

    Ok(json!(rec).to_string())
}

async fn h_search_public_chats(client: &Client, args: &Value) -> Result<String> {
    let query = str_arg(args, "query");
    if query.is_empty() {
        bail!("Missing required argument: query");
    }
    let limit = i64_arg(args, "limit", 20).clamp(1, 100) as i32;

    let result = invoke(
        client,
        &tl::functions::contacts::Search {
            q: query.to_string(),
            limit,
        },
    )
    .await
    .context("Failed to search public chats")?;

    let tl::enums::contacts::Found::Found(f) = result;
    let mut entities: Vec<Value> = f.chats.iter().map(chat_summary).collect();
    entities.extend(f.users.iter().map(user_summary));
    Ok(json!(entities).to_string())
}

async fn h_resolve_username(client: &Client, args: &Value) -> Result<String> {
    let username = str_arg(args, "username");
    if username.is_empty() {
        bail!("Missing required argument: username");
    }
    let peer = resolve_peer(client, username).await?;
    Ok(json!({
        "username": username.trim().trim_start_matches('@'),
        "id": bare_id(&peer),
        "type": kind_of(&peer),
        "name": peer_name(&peer),
    })
    .to_string())
}

async fn h_get_full_chat(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let peer = resolve_peer(client, &chat_id).await?;
    let pref = peer_ref(&peer, &chat_id).await?;

    let (about, participants_count) = match pref.id.kind() {
        PeerKind::Channel | PeerKind::Chat => {
            let res = if pref.id.kind() == PeerKind::Channel {
                invoke(
                    client,
                    &tl::functions::channels::GetFullChannel {
                        channel: pref.into(),
                    },
                )
                .await
                .context("Failed to get full channel")?
            } else {
                invoke(
                    client,
                    &tl::functions::messages::GetFullChat {
                        chat_id: pref.id.bare_id(),
                    },
                )
                .await
                .context("Failed to get full chat")?
            };
            unpack_chat_full(res)?
        }
        _ => bail!("get_full_chat only works for channels and groups, not users."),
    };

    Ok(json!({
        "id": pref.id.bare_id(),
        "title": peer_name(&peer),
        "username": peer.username(),
        "about": truncate(&about, 1024),
        "participants_count": participants_count,
    })
    .to_string())
}

async fn set_muted(client: &Client, args: &Value, muted: bool) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let peer = resolve_peer(client, &chat_id).await?;
    let pref = peer_ref(&peer, &chat_id).await?;

    let settings =
        tl::enums::InputPeerNotifySettings::Settings(tl::types::InputPeerNotifySettings {
            show_previews: None,
            silent: None,
            mute_until: Some(if muted { i32::MAX } else { 0 }),
            sound: None,
            stories_muted: None,
            stories_hide_sender: None,
            stories_sound: None,
        });
    invoke(
        client,
        &tl::functions::account::UpdateNotifySettings {
            peer: tl::enums::InputNotifyPeer::Peer(tl::types::InputNotifyPeer {
                peer: pref.into(),
            }),
            settings,
        },
    )
    .await
    .context("Failed to update notify settings")?;

    Ok(if muted {
        format!("Chat {chat_id} muted.")
    } else {
        format!("Chat {chat_id} unmuted.")
    })
}

async fn set_archived(client: &Client, args: &Value, archived: bool) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let peer = resolve_peer(client, &chat_id).await?;
    let pref = peer_ref(&peer, &chat_id).await?;

    invoke(
        client,
        &tl::functions::folders::EditPeerFolders {
            folder_peers: vec![tl::enums::InputFolderPeer::Peer(tl::types::InputFolderPeer {
                peer: pref.into(),
                folder_id: if archived { 1 } else { 0 },
            })],
        },
    )
    .await
    .context("Failed to update chat folder")?;

    Ok(if archived {
        format!("Chat {chat_id} archived.")
    } else {
        format!("Chat {chat_id} unarchived.")
    })
}

async fn h_get_common_chats(client: &Client, args: &Value) -> Result<String> {
    let user_id = id_arg(args, "user_id");
    let limit = i64_arg(args, "limit", 100).clamp(1, 100) as i32;
    let max_id = i64_arg(args, "max_id", 0);

    let peer = resolve_peer(client, &user_id).await?;
    let pref = peer_ref(&peer, &user_id).await?;
    if !matches!(pref.id.kind(), PeerKind::User | PeerKind::UserSelf) {
        bail!("get_common_chats requires a user, not a chat.");
    }

    let result = invoke(
        client,
        &tl::functions::messages::GetCommonChats {
            user_id: pref.into(),
            max_id,
            limit,
        },
    )
    .await
    .context("Failed to get common chats")?;

    let tl::enums::messages::Chats::Chats(c) = result else {
        bail!("Unexpected chats slice response");
    };
    if c.chats.is_empty() {
        return Ok(format!("No common chats found with user {user_id}."));
    }
    let chats: Vec<Value> = c.chats.iter().map(chat_summary).collect();
    Ok(json!(chats).to_string())
}

async fn h_get_message_read_by(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let message_id = i64_arg(args, "message_id", 0) as i32;

    let peer = resolve_peer(client, &chat_id).await?;
    let pref = peer_ref(&peer, &chat_id).await?;

    let readers = invoke(
        client,
        &tl::functions::messages::GetMessageReadParticipants {
            peer: pref.into(),
            msg_id: message_id,
        },
    )
    .await
    .context("Failed to get message read participants")?;

    let read_by: Vec<Value> = readers
        .iter()
        .map(|r| {
            let tl::enums::ReadParticipantDate::Date(p) = r;
            json!({ "user_id": p.user_id, "read_at_unix": p.date })
        })
        .collect();

    if read_by.is_empty() {
        return Ok(format!(
            "No read receipts available for message {message_id} in chat {chat_id}."
        ));
    }
    Ok(json!({
        "chat_id": chat_id,
        "message_id": message_id,
        "read_by": read_by,
        "count": read_by.len(),
    })
    .to_string())
}

async fn h_get_message_link(client: &Client, args: &Value) -> Result<String> {
    let chat_id = id_arg(args, "chat_id");
    let message_id = i64_arg(args, "message_id", 0) as i32;
    let thread = bool_arg(args, "thread", false);

    let peer = resolve_peer(client, &chat_id).await?;
    let pref = peer_ref(&peer, &chat_id).await?;
    if pref.id.kind() != PeerKind::Channel {
        bail!(
            "Cannot export message link for this peer type. \
             Message links are only available for channels and supergroups."
        );
    }

    let result = invoke(
        client,
        &tl::functions::channels::ExportMessageLink {
            grouped: false,
            thread,
            channel: pref.into(),
            id: message_id,
        },
    )
    .await
    .context("Failed to export message link")?;

    let tl::enums::ExportedMessageLink::Link(link) = result;
    if link.link.is_empty() {
        return Ok(format!(
            "Could not export link for message {message_id} in chat {chat_id}."
        ));
    }
    let mut out = format!("Link: {}", link.link);
    if !link.html.is_empty() {
        out.push_str(&format!("\nHTML: {}", link.html));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn schema(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
    })
}

fn chat_id_prop() -> Value {
    json!({
        "type": "string",
        "description": "Chat ID (numeric) or username (e.g. '@name' or 'name')",
    })
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_chats".into(),
            description: "Get a paginated list of chats (dialogs) with IDs and titles.".into(),
            input_schema: schema(
                json!({
                    "page": { "type": "integer", "description": "Page number (1-indexed)", "default": 1 },
                    "page_size": { "type": "integer", "description": "Chats per page", "default": 20 },
                }),
                &[],
            ),
        },
        ToolDefinition {
            name: "subscribe_public_channel".into(),
            description: "Subscribe (join) a public channel or supergroup by username or ID."
                .into(),
            input_schema: schema(
                json!({ "channel": { "type": "string", "description": "Channel username or numeric ID" } }),
                &["channel"],
            ),
        },
        ToolDefinition {
            name: "list_topics".into(),
            description: "List forum topics of a forum-enabled supergroup. Pass a topic ID as topic_id to send_* tools to post into a topic.".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "limit": { "type": "integer", "description": "Max topics to retrieve", "default": 200 },
                    "offset_topic": { "type": "integer", "description": "Topic ID offset for pagination", "default": 0 },
                    "search_query": { "type": "string", "description": "Filter topics by title" },
                }),
                &["chat_id"],
            ),
        },
        ToolDefinition {
            name: "enable_forum_topics".into(),
            description: "Enable forum topics for a supergroup. Requires admin rights to change chat info.".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "tabs": { "type": "boolean", "description": "Show topics as tabs", "default": true },
                }),
                &["chat_id"],
            ),
        },
        ToolDefinition {
            name: "create_forum_topic".into(),
            description: "Create a forum topic in a forum-enabled supergroup.".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "title": { "type": "string", "description": "Topic title" },
                    "icon_color": { "type": "integer", "description": "Topic icon color integer" },
                    "icon_emoji_id": { "type": "integer", "description": "Custom emoji document ID for the icon" },
                }),
                &["chat_id", "title"],
            ),
        },
        ToolDefinition {
            name: "edit_forum_topic".into(),
            description: "Edit a forum topic. Pass only the fields to change.".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "topic_id": { "type": "integer", "description": "ID of the topic to edit" },
                    "title": { "type": "string", "description": "New topic title" },
                    "icon_emoji_id": { "type": "integer", "description": "New custom emoji document ID (0 removes it)" },
                    "closed": { "type": "boolean", "description": "True closes the topic, False reopens it" },
                    "hidden": { "type": "boolean", "description": "True hides the General topic, False shows it" },
                }),
                &["chat_id", "topic_id"],
            ),
        },
        ToolDefinition {
            name: "delete_forum_topic".into(),
            description: "Delete a forum topic together with all its messages. Cannot be undone. The General topic (ID 1) cannot be deleted.".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "topic_id": { "type": "integer", "description": "ID of the topic to delete" },
                }),
                &["chat_id", "topic_id"],
            ),
        },
        ToolDefinition {
            name: "list_chats".into(),
            description: "List chats with metadata (type, unread, muted, archived). Use to find a private group's ID.".into(),
            input_schema: schema(
                json!({
                    "chat_type": { "type": "string", "description": "Filter by type: 'user', 'group', 'channel'" },
                    "limit": { "type": "integer", "description": "Max chats to scan", "default": 20 },
                    "unread_only": { "type": "boolean", "description": "Only chats with unread messages", "default": false },
                    "unmuted_only": { "type": "boolean", "description": "Only unmuted chats", "default": false },
                    "archived": { "type": "boolean", "description": "True: only archived, False: only non-archived, omit: all" },
                    "with_about": { "type": "boolean", "description": "Fetch each chat's description (one extra API call per chat)", "default": false },
                }),
                &[],
            ),
        },
        ToolDefinition {
            name: "get_chat".into(),
            description: "Get detailed information about a specific chat by ID or username.".into(),
            input_schema: schema(json!({ "chat_id": chat_id_prop() }), &["chat_id"]),
        },
        ToolDefinition {
            name: "search_public_chats".into(),
            description: "Search for public chats, channels, or bots by username or title.".into(),
            input_schema: schema(
                json!({
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Max results", "default": 20 },
                }),
                &["query"],
            ),
        },
        ToolDefinition {
            name: "resolve_username".into(),
            description: "Resolve a username to a user or chat ID.".into(),
            input_schema: schema(
                json!({ "username": { "type": "string", "description": "Username with or without @" } }),
                &["username"],
            ),
        },
        ToolDefinition {
            name: "get_full_chat".into(),
            description: "Get full info of a channel or group, including description/about text.".into(),
            input_schema: schema(json!({ "chat_id": chat_id_prop() }), &["chat_id"]),
        },
        ToolDefinition {
            name: "mute_chat".into(),
            description: "Mute notifications for a chat.".into(),
            input_schema: schema(json!({ "chat_id": chat_id_prop() }), &["chat_id"]),
        },
        ToolDefinition {
            name: "unmute_chat".into(),
            description: "Unmute notifications for a chat.".into(),
            input_schema: schema(json!({ "chat_id": chat_id_prop() }), &["chat_id"]),
        },
        ToolDefinition {
            name: "archive_chat".into(),
            description: "Archive a chat.".into(),
            input_schema: schema(json!({ "chat_id": chat_id_prop() }), &["chat_id"]),
        },
        ToolDefinition {
            name: "unarchive_chat".into(),
            description: "Unarchive a chat.".into(),
            input_schema: schema(json!({ "chat_id": chat_id_prop() }), &["chat_id"]),
        },
        ToolDefinition {
            name: "get_common_chats".into(),
            description: "List chats shared with a specific user.".into(),
            input_schema: schema(
                json!({
                    "user_id": { "type": "string", "description": "User ID or username" },
                    "limit": { "type": "integer", "description": "Max chats (max 100)", "default": 100 },
                    "max_id": { "type": "integer", "description": "Pagination cursor: last chat ID of previous page", "default": 0 },
                }),
                &["user_id"],
            ),
        },
        ToolDefinition {
            name: "get_message_read_by".into(),
            description: "List user IDs who have read a specific message (small groups/supergroups, recent messages only).".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "message_id": { "type": "integer", "description": "Message ID to check read receipts for" },
                }),
                &["chat_id", "message_id"],
            ),
        },
        ToolDefinition {
            name: "get_message_link".into(),
            description: "Export a t.me/... link for a message. Only works on channels and supergroups.".into(),
            input_schema: schema(
                json!({
                    "chat_id": chat_id_prop(),
                    "message_id": { "type": "integer", "description": "Message ID to export a link for" },
                    "thread": { "type": "boolean", "description": "Link opens the message inside its discussion thread", "default": false },
                }),
                &["chat_id", "message_id"],
            ),
        },
    ]
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<Option<CallToolResult>> {
    let text = match tool_name {
        "get_chats" => h_get_chats(client, args).await?,
        "subscribe_public_channel" => h_subscribe_public_channel(client, args).await?,
        "list_topics" => h_list_topics(client, args).await?,
        "enable_forum_topics" => h_enable_forum_topics(client, args).await?,
        "create_forum_topic" => h_create_forum_topic(client, args).await?,
        "edit_forum_topic" => h_edit_forum_topic(client, args).await?,
        "delete_forum_topic" => h_delete_forum_topic(client, args).await?,
        "list_chats" => h_list_chats(client, args).await?,
        "get_chat" => h_get_chat(client, args).await?,
        "search_public_chats" => h_search_public_chats(client, args).await?,
        "resolve_username" => h_resolve_username(client, args).await?,
        "get_full_chat" => h_get_full_chat(client, args).await?,
        "mute_chat" => set_muted(client, args, true).await?,
        "unmute_chat" => set_muted(client, args, false).await?,
        "archive_chat" => set_archived(client, args, true).await?,
        "unarchive_chat" => set_archived(client, args, false).await?,
        "get_common_chats" => h_get_common_chats(client, args).await?,
        "get_message_read_by" => h_get_message_read_by(client, args).await?,
        "get_message_link" => h_get_message_link(client, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(CallToolResult::ok(text)))
}
