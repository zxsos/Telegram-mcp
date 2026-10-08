//! Folder (dialog filter) management tools.
//!
//! Peer lists take **bare numeric chat ids** (as returned by `list_chats`)
//! or `@username` strings where noted. Saved Messages is addressed with
//! your own user id. Titles keep their raw text; title entity formatting
//! (rich text) is not supported by this build.

use super::{bool_arg, i64_arg, str_arg};
use crate::mcp::{CallToolResult, ToolDefinition};
use anyhow::{Context, bail};
use grammers_client::Client;
use grammers_client::peer::Peer;
use grammers_tl_types as tl;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

const FOLDER_FLAGS: &[&str] = &[
    "contacts",
    "non_contacts",
    "groups",
    "broadcasts",
    "bots",
    "exclude_muted",
    "exclude_read",
    "exclude_archived",
    "title_noanimate",
];
const PEER_FIELDS: &[&str] = &["include_chat_ids", "pinned_chat_ids", "exclude_chat_ids"];
/// Telegram's official client title limit, in UTF-16 code units.
const TITLE_LIMIT_UTF16: usize = 12;

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

async fn dialog_filters(client: &Client) -> anyhow::Result<tl::types::messages::DialogFilters> {
    let res = client
        .invoke(&tl::functions::messages::GetDialogFilters {})
        .await
        .context("messages.getDialogFilters failed")?;
    match res {
        tl::enums::messages::DialogFilters::Filters(f) => Ok(f),
    }
}

fn title_text(title: &tl::enums::TextWithEntities) -> &str {
    match title {
        tl::enums::TextWithEntities::Entities(t) => &t.text,
    }
}

fn folder_id_of(f: &tl::enums::DialogFilter) -> i32 {
    match f {
        tl::enums::DialogFilter::Filter(f) => f.id,
        tl::enums::DialogFilter::Chatlist(f) => f.id,
        tl::enums::DialogFilter::Default => 0,
    }
}

async fn self_id(client: &Client) -> anyhow::Result<i64> {
    Ok(client
        .get_me()
        .await
        .context("Failed to get own identity")?
        .id()
        .bare_id())
}

/// (kind, bare id) identity used to compare peers.
fn peer_key(peer: &tl::enums::InputPeer, self_id: i64) -> (u8, i64) {
    match peer {
        tl::enums::InputPeer::Empty => (0, 0),
        tl::enums::InputPeer::PeerSelf => (1, self_id),
        tl::enums::InputPeer::User(u) => (2, u.user_id),
        tl::enums::InputPeer::Chat(c) => (3, c.chat_id),
        tl::enums::InputPeer::Channel(c) => (4, c.channel_id),
        tl::enums::InputPeer::UserFromMessage(u) => (2, u.user_id),
        tl::enums::InputPeer::ChannelFromMessage(c) => (4, c.channel_id),
    }
}

fn bare_peer_ids(peers: &[tl::enums::InputPeer], self_id: i64) -> Vec<i64> {
    peers.iter().map(|p| peer_key(p, self_id).1).collect()
}

fn peer_key_set(peers: &[tl::enums::InputPeer], self_id: i64) -> HashSet<(u8, i64)> {
    peers.iter().map(|p| peer_key(p, self_id)).collect()
}

struct DialogEntry {
    peer: tl::enums::InputPeer,
    name: String,
    kind: &'static str,
    username: Option<String>,
}

/// Index dialogs by bare id for peer resolution and name lookup.
async fn dialog_map(client: &Client) -> anyhow::Result<HashMap<i64, DialogEntry>> {
    let mut map = HashMap::new();
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.context("Failed to iterate dialogs")? {
        let peer = dialog.peer();
        let kind = match peer {
            Peer::User(_) => "user",
            Peer::Group(_) => "group",
            Peer::Channel(_) => "channel",
        };
        map.insert(
            peer.id().bare_id(),
            DialogEntry {
                peer: tl::enums::InputPeer::from(dialog.peer_ref()),
                name: peer.name().unwrap_or("(no name)").to_string(),
                kind,
                username: peer.username().map(str::to_string),
            },
        );
    }
    Ok(map)
}

/// Resolve a `chat_id` argument (bare numeric id or `@username`) to an `InputPeer`.
async fn resolve_input_peer(
    client: &Client,
    chat_id: &Value,
    dialogs: &HashMap<i64, DialogEntry>,
    me: i64,
) -> anyhow::Result<tl::enums::InputPeer> {
    if let Some(n) = chat_id.as_i64() {
        if n == me {
            return Ok(tl::enums::InputPeer::PeerSelf);
        }
        if let Some(entry) = dialogs.get(&n) {
            return Ok(entry.peer.clone());
        }
        bail!("Failed to resolve chat {n}. Use list_chats to find IDs.");
    }
    if let Some(s) = chat_id.as_str() {
        let username = s.strip_prefix('@').unwrap_or(s);
        let peer = client
            .resolve_username(username)
            .await
            .context("Failed to resolve username")?
            .with_context(|| format!("No user/channel found for @{username}"))?;
        let peer_ref = peer.to_ref().await.context("Failed to get peer ref")?;
        return Ok(tl::enums::InputPeer::from(peer_ref));
    }
    bail!("chat_id must be a numeric chat ID or a @username string")
}

// Deterministic FNV-1a revision of a canonical folder state (no extra deps).
fn revision(canonical: &Value) -> String {
    let text = serde_json::to_string(canonical).unwrap_or_default();
    let mut h: u64 = 0x0cbf_29ce_4842_2225;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

fn private_definition(f: &tl::types::DialogFilter, me: i64) -> Value {
    let mut include = bare_peer_ids(&f.include_peers, me);
    let mut exclude = bare_peer_ids(&f.exclude_peers, me);
    include.sort_unstable();
    exclude.sort_unstable();
    json!({
        "title": title_text(&f.title),
        "emoticon": f.emoticon,
        "color": f.color,
        "title_noanimate": f.title_noanimate,
        "contacts": f.contacts,
        "non_contacts": f.non_contacts,
        "groups": f.groups,
        "broadcasts": f.broadcasts,
        "bots": f.bots,
        "exclude_muted": f.exclude_muted,
        "exclude_read": f.exclude_read,
        "exclude_archived": f.exclude_archived,
        "include_chat_ids": include,
        "pinned_chat_ids": bare_peer_ids(&f.pinned_peers, me),
        "exclude_chat_ids": exclude,
    })
}

fn folder_state(f: &tl::enums::DialogFilter, me: i64) -> Value {
    match f {
        tl::enums::DialogFilter::Default => json!({"id": 0, "type": "system", "editable": false}),
        tl::enums::DialogFilter::Chatlist(c) => {
            let mut include = bare_peer_ids(&c.include_peers, me);
            include.sort_unstable();
            let canonical = json!({
                "id": c.id, "type": "shared",
                "definition": {
                    "title": title_text(&c.title),
                    "emoticon": c.emoticon,
                    "color": c.color,
                    "title_noanimate": c.title_noanimate,
                    "has_my_invites": c.has_my_invites,
                    "include_chat_ids": include,
                    "pinned_chat_ids": bare_peer_ids(&c.pinned_peers, me),
                },
            });
            let rev = revision(&canonical);
            json!({
                "id": c.id, "type": "shared", "editable": false,
                "has_my_invites": c.has_my_invites,
                "definition": canonical["definition"],
                "revision": rev,
            })
        }
        tl::enums::DialogFilter::Filter(f) => {
            let definition = private_definition(f, me);
            let canonical = json!({"id": f.id, "type": "private", "definition": definition});
            let rev = revision(&canonical);
            json!({
                "id": f.id, "type": "private", "editable": true,
                "definition": definition,
                "revision": rev,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// folder limits
// ---------------------------------------------------------------------------

struct FolderLimits {
    premium: Option<bool>,
    config_available: bool,
    folders: Option<i64>,
    chats_per_folder: Option<i64>,
    pinned_per_folder: Option<i64>,
}

async fn read_limits(client: &Client) -> FolderLimits {
    let premium = client.get_me().await.ok().map(|u| u.is_premium());
    let tier = match premium {
        Some(true) => "premium",
        Some(false) => "default",
        None => "",
    };
    let mut values: HashMap<String, i64> = HashMap::new();
    let mut config_available = false;
    if let Ok(res) = client
        .invoke(&tl::functions::help::GetAppConfig { hash: 0 })
        .await
        && let tl::enums::help::AppConfig::Config(cfg) = res
        && let tl::enums::Jsonvalue::JsonObject(obj) = cfg.config
    {
        config_available = true;
        for entry in obj.value {
            if let tl::enums::JsonobjectValue::JsonObjectValue(e) = entry
                && let tl::enums::Jsonvalue::JsonNumber(n) = e.value
                && n.value > 0.0
                && n.value.fract() == 0.0
            {
                values.insert(e.key, n.value as i64);
            }
        }
    }
    let get = |base: &str| {
        if tier.is_empty() {
            None
        } else {
            values.get(&format!("{base}_{tier}")).copied()
        }
    };
    FolderLimits {
        premium,
        config_available,
        folders: get("dialog_filters_limit"),
        chats_per_folder: get("dialog_filters_chats_limit"),
        pinned_per_folder: get("dialogs_folder_pinned_limit"),
    }
}

async fn get_folder_limits(client: &Client) -> anyhow::Result<String> {
    let limits = read_limits(client).await;
    Ok(serde_json::to_string_pretty(&json!({
        "premium": limits.premium,
        "config_available": limits.config_available,
        "limits": {
            "folders": limits.folders,
            "chats_per_folder": limits.chats_per_folder,
            "pinned_per_folder": limits.pinned_per_folder,
        },
        "title_limit_utf16": TITLE_LIMIT_UTF16,
        "unknown_limits": "Unknown limits defer to Telegram's server validation.",
    }))?)
}

// ---------------------------------------------------------------------------
// snapshot
// ---------------------------------------------------------------------------

async fn get_folder_snapshot(client: &Client) -> anyhow::Result<String> {
    let filters = dialog_filters(client).await?;
    let me = self_id(client).await?;
    let folders: Vec<Value> = filters
        .filters
        .iter()
        .map(|f| folder_state(f, me))
        .collect();
    let order: Vec<i32> = folders
        .iter()
        .filter_map(|s| s.get("id").and_then(Value::as_i64).map(|id| id as i32))
        .collect();
    Ok(serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "folders": folders,
        "folder_order": order,
        "tags_enabled": filters.tags_enabled,
        "scope": "folder definitions only; shared invite state and chat state are not restorable",
    }))?)
}

// ---------------------------------------------------------------------------
// update_folder
// ---------------------------------------------------------------------------

fn validate_patch(patch: &Map<String, Value>) -> anyhow::Result<()> {
    let mut allowed: HashSet<&str> = HashSet::new();
    allowed.extend(FOLDER_FLAGS.iter().copied());
    allowed.extend(PEER_FIELDS.iter().copied());
    allowed.extend(["title", "title_entities", "emoticon", "color"]);
    for key in patch.keys() {
        if !allowed.contains(key.as_str()) {
            bail!("Error: patch contains unsupported folder field '{key}'.");
        }
    }
    if let Some(t) = patch.get("title") {
        let s = t.as_str().context("Error: title must be text.")?;
        if s.trim().is_empty() || s.encode_utf16().count() > TITLE_LIMIT_UTF16 {
            bail!("Error: title must be nonempty and at most 12 UTF-16 units.");
        }
    }
    for flag in FOLDER_FLAGS {
        if let Some(v) = patch.get(*flag)
            && !v.is_null()
            && !v.is_boolean()
        {
            bail!("Error: folder rules must be boolean or null.");
        }
    }
    if let Some(v) = patch.get("color")
        && !v.is_null()
    {
        let n = v
            .as_i64()
            .context("Error: folder color must be -1..6 or null.")?;
        if !(-1..=6).contains(&n) {
            bail!("Error: folder color must be -1..6 or null.");
        }
    }
    if let Some(v) = patch.get("emoticon")
        && !v.is_null()
        && !v.is_string()
    {
        bail!("Error: emoticon must be text or null.");
    }
    for name in PEER_FIELDS {
        if let Some(v) = patch.get(*name) {
            let arr = v
                .as_array()
                .with_context(|| format!("Error: {name} must be a list of chat IDs."))?;
            let mut seen = HashSet::new();
            for id in arr {
                let n = id
                    .as_i64()
                    .filter(|n| *n != 0)
                    .context("Error: peer lists require unique, nonzero integer chat IDs.")?;
                if !seen.insert(n) {
                    bail!("Error: peer lists require unique chat IDs (duplicate {n}).");
                }
            }
        }
    }
    Ok(())
}

async fn apply_patch(
    updated: &mut tl::types::DialogFilter,
    patch: &Map<String, Value>,
    client: &Client,
    dialogs: &HashMap<i64, DialogEntry>,
    me: i64,
) -> anyhow::Result<()> {
    if let Some(t) = patch.get("title") {
        let text = t.as_str().unwrap_or("").to_string();
        updated.title = tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
            text,
            entities: vec![],
        });
    }
    if let Some(e) = patch.get("title_entities") {
        if e.as_array().is_some_and(|a| !a.is_empty()) {
            // TODO: parse TL message-entity dictionaries. Rich title formatting
            // is not supported by this build yet.
            bail!("not supported: title_entities with entries");
        }
        let tl::enums::TextWithEntities::Entities(t) = &mut updated.title;
        t.entities.clear();
    }
    let set_flag = |updated: &mut tl::types::DialogFilter, name: &str, value: bool| match name {
        "contacts" => updated.contacts = value,
        "non_contacts" => updated.non_contacts = value,
        "groups" => updated.groups = value,
        "broadcasts" => updated.broadcasts = value,
        "bots" => updated.bots = value,
        "exclude_muted" => updated.exclude_muted = value,
        "exclude_read" => updated.exclude_read = value,
        "exclude_archived" => updated.exclude_archived = value,
        "title_noanimate" => updated.title_noanimate = value,
        _ => {}
    };
    for flag in FOLDER_FLAGS {
        if let Some(v) = patch.get(*flag) {
            set_flag(updated, flag, v.as_bool().unwrap_or(false));
        }
    }
    if let Some(v) = patch.get("emoticon") {
        updated.emoticon = v.as_str().map(str::to_string);
    }
    if let Some(v) = patch.get("color") {
        updated.color = v.as_i64().map(|n| n as i32);
    }
    for (name, slot) in [
        ("include_chat_ids", &mut updated.include_peers),
        ("pinned_chat_ids", &mut updated.pinned_peers),
        ("exclude_chat_ids", &mut updated.exclude_peers),
    ] {
        if let Some(v) = patch.get(name) {
            let mut peers = Vec::new();
            for id in v.as_array().cloned().unwrap_or_default() {
                let n = id.as_i64().unwrap_or(0);
                peers.push(
                    resolve_input_peer(client, &id, dialogs, me)
                        .await
                        .with_context(|| format!("Failed to resolve peer id {n} in '{name}'"))?,
                );
            }
            *slot = peers;
        }
    }
    Ok(())
}

fn find_folder(
    filters: &[tl::enums::DialogFilter],
    folder_id: i32,
) -> Option<&tl::enums::DialogFilter> {
    filters.iter().find(|f| folder_id_of(f) == folder_id)
}

async fn update_folder(client: &Client, args: &Value) -> anyhow::Result<String> {
    let folder_id = i64_arg(args, "folder_id", 0);
    if !(2..(1i64 << 31)).contains(&folder_id) {
        bail!("Error: folder_id must be an existing private folder ID (2..2147483647).");
    }
    let folder_id = folder_id as i32;
    let patch = args
        .get("patch")
        .and_then(Value::as_object)
        .context("Error: patch must be an object.")?;
    validate_patch(patch)?;
    let expected_revision = args.get("expected_revision").and_then(Value::as_str);

    let me = self_id(client).await?;
    let filters = dialog_filters(client).await?;
    let current = find_folder(&filters.filters, folder_id)
        .with_context(|| format!("Error: folder {folder_id} not found."))?;
    let tl::enums::DialogFilter::Filter(current) = current else {
        bail!("Error: shared folders cannot be edited by update_folder.");
    };

    let before = revision(
        &json!({"id": current.id, "type": "private", "definition": private_definition(current, me)}),
    );
    if let Some(exp) = expected_revision
        && exp != before
    {
        bail!("Error: folder changed since the snapshot; read a fresh snapshot first.");
    }

    let dialogs = dialog_map(client).await?;
    let mut updated = current.clone();
    apply_patch(&mut updated, patch, client, &dialogs, me).await?;

    let has_inclusion = !updated.include_peers.is_empty()
        || !updated.pinned_peers.is_empty()
        || updated.contacts
        || updated.non_contacts
        || updated.groups
        || updated.broadcasts
        || updated.bots;
    if !has_inclusion {
        bail!("Error: folder needs at least one included peer or inclusion rule.");
    }

    let included = peer_key_set(&updated.include_peers, me)
        .union(&peer_key_set(&updated.pinned_peers, me))
        .copied()
        .collect::<HashSet<_>>();
    let excluded = peer_key_set(&updated.exclude_peers, me);
    if !included.is_disjoint(&excluded) {
        bail!("Error: included/pinned and excluded peer lists must not overlap.");
    }

    let limits = read_limits(client).await;
    if let Some(limit) = limits.chats_per_folder
        && (included.len() > limit as usize || excluded.len() > limit as usize)
    {
        bail!("Error: explicit peer count exceeds the configured per-folder limit ({limit}).");
    }
    if let Some(limit) = limits.pinned_per_folder
        && updated.pinned_peers.len() > limit as usize
    {
        bail!("Error: pinned peer count exceeds the configured per-folder limit ({limit}).");
    }

    let after = revision(
        &json!({"id": updated.id, "type": "private", "definition": private_definition(&updated, me)}),
    );
    if before == after {
        return Ok(serde_json::to_string_pretty(&json!({
            "success": true, "folder_id": folder_id,
            "changed": false, "revision": before,
        }))?);
    }

    // Re-read to detect intervening changes (no server-side compare-and-swap).
    let latest = dialog_filters(client).await?;
    match find_folder(&latest.filters, folder_id) {
        Some(tl::enums::DialogFilter::Filter(cur))
            if revision(
                &json!({"id": cur.id, "type": "private", "definition": private_definition(cur, me)}),
            ) == before => {}
        _ => {
            bail!("Error: folder changed while preparing the update; read a fresh snapshot first.")
        }
    }

    client
        .invoke(&tl::functions::messages::UpdateDialogFilter {
            id: folder_id,
            filter: Some(tl::enums::DialogFilter::Filter(updated)),
        })
        .await
        .context("messages.updateDialogFilter failed")?;

    Ok(serde_json::to_string_pretty(&json!({
        "success": true, "folder_id": folder_id,
        "changed": true, "revision": after,
    }))?)
}

// ---------------------------------------------------------------------------
// list_folders / get_folder
// ---------------------------------------------------------------------------

async fn list_folders(client: &Client) -> anyhow::Result<String> {
    let filters = dialog_filters(client).await?;
    let mut folders = Vec::new();
    for f in &filters.filters {
        match f {
            tl::enums::DialogFilter::Default => {}
            tl::enums::DialogFilter::Filter(fl) => folders.push(json!({
                "id": fl.id,
                "title": title_text(&fl.title),
                "emoticon": fl.emoticon,
                "contacts": fl.contacts,
                "non_contacts": fl.non_contacts,
                "groups": fl.groups,
                "broadcasts": fl.broadcasts,
                "bots": fl.bots,
                "exclude_muted": fl.exclude_muted,
                "exclude_read": fl.exclude_read,
                "exclude_archived": fl.exclude_archived,
                "included_peers_count": fl.include_peers.len(),
                "excluded_peers_count": fl.exclude_peers.len(),
                "pinned_peers_count": fl.pinned_peers.len(),
            })),
            tl::enums::DialogFilter::Chatlist(c) => folders.push(json!({
                "id": c.id,
                "title": title_text(&c.title),
                "emoticon": c.emoticon,
                "type": "shared",
                "included_peers_count": c.include_peers.len(),
                "pinned_peers_count": c.pinned_peers.len(),
            })),
        }
    }
    if folders.is_empty() {
        return Ok("No folders found. Create one with create_folder tool.".to_string());
    }
    Ok(serde_json::to_string_pretty(&json!({
        "folders": folders,
        "count": folders.len(),
    }))?)
}

fn peer_chat_info(
    peer: &tl::enums::InputPeer,
    me: i64,
    dialogs: &HashMap<i64, DialogEntry>,
) -> Value {
    let key = peer_key(peer, me);
    if let Some(entry) = dialogs.get(&key.1) {
        let mut info = json!({
            "id": key.1,
            "name": entry.name,
            "type": entry.kind,
        });
        if let Some(u) = &entry.username {
            info["username"] = u.clone().into();
        }
        info
    } else {
        json!({"id": key.1, "name": "Unknown", "type": "Unknown"})
    }
}

async fn get_folder(client: &Client, args: &Value) -> anyhow::Result<String> {
    let folder_id = i64_arg(args, "folder_id", 0) as i32;
    let filters = dialog_filters(client).await?;
    let target = find_folder(&filters.filters, folder_id).with_context(|| {
        format!("Folder with ID {folder_id} not found. Use list_folders to see available folders.")
    })?;

    let me = self_id(client).await?;
    let dialogs = dialog_map(client).await?;
    let info = |peers: &[tl::enums::InputPeer]| -> Vec<Value> {
        peers
            .iter()
            .map(|p| peer_chat_info(p, me, &dialogs))
            .collect()
    };

    let data = match target {
        tl::enums::DialogFilter::Default => bail!("Folder {folder_id} is the system folder."),
        tl::enums::DialogFilter::Filter(f) => json!({
            "id": folder_id,
            "title": title_text(&f.title),
            "emoticon": f.emoticon,
            "included_chats": info(&f.include_peers),
            "excluded_chats": info(&f.exclude_peers),
            "pinned_chats": info(&f.pinned_peers),
            "filters": {
                "contacts": f.contacts,
                "non_contacts": f.non_contacts,
                "groups": f.groups,
                "broadcasts": f.broadcasts,
                "bots": f.bots,
                "exclude_muted": f.exclude_muted,
                "exclude_read": f.exclude_read,
                "exclude_archived": f.exclude_archived,
            },
        }),
        tl::enums::DialogFilter::Chatlist(c) => json!({
            "id": folder_id,
            "title": title_text(&c.title),
            "emoticon": c.emoticon,
            "type": "shared",
            "included_chats": info(&c.include_peers),
            "excluded_chats": [],
            "pinned_chats": info(&c.pinned_peers),
        }),
    };
    Ok(serde_json::to_string_pretty(&data)?)
}

// ---------------------------------------------------------------------------
// create_folder
// ---------------------------------------------------------------------------

async fn create_folder(client: &Client, args: &Value) -> anyhow::Result<String> {
    let title = str_arg(args, "title");
    if title.trim().is_empty() {
        bail!("Error: title is required and must be nonempty.");
    }
    if title.encode_utf16().count() > TITLE_LIMIT_UTF16 {
        bail!("Error: title must be at most 12 UTF-16 units.");
    }

    let filters = dialog_filters(client).await?;
    let mut existing: HashSet<i32> = HashSet::new();
    let mut count = 0;
    for f in &filters.filters {
        if let tl::enums::DialogFilter::Default = f {
            continue;
        }
        existing.insert(folder_id_of(f));
        count += 1;
    }
    if let Some(limit) = read_limits(client).await.folders
        && count >= limit as usize
    {
        bail!(
            "Cannot create folder: you've reached Telegram's folder limit of {limit} for your account. Delete a folder first."
        );
    }
    let mut new_id: i32 = 2;
    while existing.contains(&new_id) {
        new_id += 1;
    }

    let me = self_id(client).await?;
    let dialogs = dialog_map(client).await?;
    let mut include_peers = Vec::new();
    for chat in args
        .get("chat_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        include_peers.push(resolve_input_peer(client, &chat, &dialogs, me).await?);
    }

    let new_filter = tl::types::DialogFilter {
        contacts: bool_arg(args, "contacts", false),
        non_contacts: bool_arg(args, "non_contacts", false),
        groups: bool_arg(args, "groups", false),
        broadcasts: bool_arg(args, "broadcasts", false),
        bots: bool_arg(args, "bots", false),
        exclude_muted: bool_arg(args, "exclude_muted", false),
        exclude_read: bool_arg(args, "exclude_read", false),
        exclude_archived: bool_arg(args, "exclude_archived", true),
        title_noanimate: false,
        id: new_id,
        title: tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
            text: title.to_string(),
            entities: vec![],
        }),
        emoticon: args
            .get("emoticon")
            .and_then(Value::as_str)
            .map(str::to_string),
        color: None,
        pinned_peers: vec![],
        include_peers,
        exclude_peers: vec![],
    };

    client
        .invoke(&tl::functions::messages::UpdateDialogFilter {
            id: new_id,
            filter: Some(tl::enums::DialogFilter::Filter(new_filter)),
        })
        .await
        .context("messages.updateDialogFilter failed")?;

    Ok(serde_json::to_string_pretty(&json!({
        "success": true,
        "folder_id": new_id,
        "title": title,
        "emoticon": args.get("emoticon").and_then(Value::as_str),
        "included_chats_count": args.get("chat_ids").and_then(Value::as_array).map_or(0, Vec::len),
    }))?)
}

// ---------------------------------------------------------------------------
// add / remove chat
// ---------------------------------------------------------------------------

fn rebuild_private(
    f: &tl::types::DialogFilter,
    include: Vec<tl::enums::InputPeer>,
    pinned: Vec<tl::enums::InputPeer>,
) -> tl::types::DialogFilter {
    let mut u = f.clone();
    u.include_peers = include;
    u.pinned_peers = pinned;
    u
}

async fn add_chat_to_folder(client: &Client, args: &Value) -> anyhow::Result<String> {
    let folder_id = i64_arg(args, "folder_id", 0) as i32;
    let chat_id = args.get("chat_id").cloned().unwrap_or(Value::Null);
    let pinned = bool_arg(args, "pinned", false);

    let filters = dialog_filters(client).await?;
    let target = find_folder(&filters.filters, folder_id).with_context(|| {
        format!("Folder with ID {folder_id} not found. Use list_folders to see available folders.")
    })?;

    let me = self_id(client).await?;
    let dialogs = dialog_map(client).await?;
    let peer = resolve_input_peer(client, &chat_id, &dialogs, me).await?;
    let key = peer_key(&peer, me);

    let (include, pinned_list, shared) = match target {
        tl::enums::DialogFilter::Default => bail!("Folder {folder_id} is the system folder."),
        tl::enums::DialogFilter::Filter(f) => {
            (f.include_peers.clone(), f.pinned_peers.clone(), false)
        }
        tl::enums::DialogFilter::Chatlist(c) => {
            (c.include_peers.clone(), c.pinned_peers.clone(), true)
        }
    };
    let already_included = include.iter().any(|p| peer_key(p, me) == key);
    let already_pinned = pinned_list.iter().any(|p| peer_key(p, me) == key);
    if already_included && (!pinned || already_pinned) {
        return Ok(format!("Chat {chat_id} is already in folder {folder_id}."));
    }

    let mut include = include;
    let mut pinned_list = pinned_list;
    if !already_included {
        include.push(peer.clone());
    }
    if pinned && !already_pinned {
        pinned_list.push(peer);
    }

    let updated = match target {
        tl::enums::DialogFilter::Filter(f) => {
            tl::enums::DialogFilter::Filter(rebuild_private(f, include, pinned_list))
        }
        tl::enums::DialogFilter::Chatlist(c) => {
            let mut u = c.clone();
            u.include_peers = include;
            u.pinned_peers = pinned_list;
            tl::enums::DialogFilter::Chatlist(u)
        }
        tl::enums::DialogFilter::Default => unreachable!(),
    };

    client
        .invoke(&tl::functions::messages::UpdateDialogFilter {
            id: folder_id,
            filter: Some(updated),
        })
        .await
        .context("messages.updateDialogFilter failed")?;

    let _ = shared;
    Ok(format!(
        "Chat {chat_id} added to folder {folder_id}{}.",
        if pinned { " (pinned)" } else { "" }
    ))
}

async fn remove_chat_from_folder(client: &Client, args: &Value) -> anyhow::Result<String> {
    let folder_id = i64_arg(args, "folder_id", 0) as i32;
    let chat_id = args.get("chat_id").cloned().unwrap_or(Value::Null);

    let filters = dialog_filters(client).await?;
    let target = find_folder(&filters.filters, folder_id).with_context(|| {
        format!("Folder with ID {folder_id} not found. Use list_folders to see available folders.")
    })?;

    let me = self_id(client).await?;
    let dialogs = dialog_map(client).await?;
    let peer = resolve_input_peer(client, &chat_id, &dialogs, me).await?;
    let key = peer_key(&peer, me);

    let keep = |p: &tl::enums::InputPeer| peer_key(p, me) != key;
    let (include, pinned_list) = match target {
        tl::enums::DialogFilter::Default => bail!("Folder {folder_id} is the system folder."),
        tl::enums::DialogFilter::Filter(f) => (
            f.include_peers
                .iter()
                .filter(|p| keep(p))
                .cloned()
                .collect::<Vec<_>>(),
            f.pinned_peers
                .iter()
                .filter(|p| keep(p))
                .cloned()
                .collect::<Vec<_>>(),
        ),
        tl::enums::DialogFilter::Chatlist(c) => (
            c.include_peers
                .iter()
                .filter(|p| keep(p))
                .cloned()
                .collect::<Vec<_>>(),
            c.pinned_peers
                .iter()
                .filter(|p| keep(p))
                .cloned()
                .collect::<Vec<_>>(),
        ),
    };
    let before_count = |f: &tl::enums::DialogFilter| match f {
        tl::enums::DialogFilter::Filter(f) => f.include_peers.len() + f.pinned_peers.len(),
        tl::enums::DialogFilter::Chatlist(c) => c.include_peers.len() + c.pinned_peers.len(),
        tl::enums::DialogFilter::Default => 0,
    };
    if include.len() + pinned_list.len() == before_count(target) {
        return Ok(format!("Chat {chat_id} was not in folder {folder_id}."));
    }

    let updated = match target {
        tl::enums::DialogFilter::Filter(f) => {
            tl::enums::DialogFilter::Filter(rebuild_private(f, include, pinned_list))
        }
        tl::enums::DialogFilter::Chatlist(c) => {
            let mut u = c.clone();
            u.include_peers = include;
            u.pinned_peers = pinned_list;
            tl::enums::DialogFilter::Chatlist(u)
        }
        tl::enums::DialogFilter::Default => unreachable!(),
    };

    client
        .invoke(&tl::functions::messages::UpdateDialogFilter {
            id: folder_id,
            filter: Some(updated),
        })
        .await
        .context("messages.updateDialogFilter failed")?;

    Ok(format!("Chat {chat_id} removed from folder {folder_id}."))
}

// ---------------------------------------------------------------------------
// delete / reorder
// ---------------------------------------------------------------------------

async fn delete_folder(client: &Client, args: &Value) -> anyhow::Result<String> {
    let folder_id = i64_arg(args, "folder_id", 0) as i32;
    if folder_id < 2 {
        bail!("Cannot delete system folder (ID {folder_id}). Only custom folders can be deleted.");
    }
    let filters = dialog_filters(client).await?;
    let title = match find_folder(&filters.filters, folder_id) {
        Some(tl::enums::DialogFilter::Default) | None => {
            bail!("Folder with ID {folder_id} not found (may already be deleted).")
        }
        Some(f) => match f {
            tl::enums::DialogFilter::Filter(f) => title_text(&f.title).to_string(),
            tl::enums::DialogFilter::Chatlist(c) => title_text(&c.title).to_string(),
            tl::enums::DialogFilter::Default => unreachable!(),
        },
    };
    client
        .invoke(&tl::functions::messages::UpdateDialogFilter {
            id: folder_id,
            filter: None,
        })
        .await
        .context("messages.updateDialogFilter failed")?;
    Ok(format!(
        "Folder '{title}' (ID {folder_id}) deleted. Chats are preserved."
    ))
}

async fn reorder_folders(client: &Client, args: &Value) -> anyhow::Result<String> {
    let ids: Vec<i32> = args
        .get("folder_ids")
        .and_then(Value::as_array)
        .context("folder_ids must be a list of folder IDs.")?
        .iter()
        .map(|v| v.as_i64().unwrap_or(-1) as i32)
        .collect();

    let filters = dialog_filters(client).await?;
    let mut existing: HashSet<i32> = HashSet::new();
    for f in &filters.filters {
        if !matches!(f, tl::enums::DialogFilter::Default) {
            existing.insert(folder_id_of(f));
        }
    }
    for id in &ids {
        if !existing.contains(id) {
            bail!("Folder ID {id} not found. Use list_folders to see available folders.");
        }
    }
    let given: HashSet<i32> = ids.iter().copied().collect();
    if given != existing {
        let missing: Vec<i32> = existing.difference(&given).copied().collect();
        bail!("All folder IDs must be included. Missing: {missing:?}");
    }

    client
        .invoke(&tl::functions::messages::UpdateDialogFiltersOrder { order: ids.clone() })
        .await
        .context("messages.updateDialogFiltersOrder failed")?;
    Ok(format!("Folders reordered: {ids:?}"))
}

// ---------------------------------------------------------------------------
// definitions + dispatch
// ---------------------------------------------------------------------------

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_folder_limits".into(),
            description: "Read effective Premium-aware folder, explicit-chat and pin limits from app config. Missing/unusable values are null, never invented defaults. Title limit uses UTF-16 units.".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "get_folder_snapshot".into(),
            description: "Read complete folder definitions and order without chat content. Private definition objects can be passed back as update_folder patches to restore state. Chat ids are bare numeric ids (see list_chats).".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "update_folder".into(),
            description: "Patch an existing PRIVATE folder by ID, preserving every omitted field. patch fields: title, emoticon, color (-1..6), title_noanimate, contacts, non_contacts, groups, broadcasts, bots, exclude_muted, exclude_read, exclude_archived, include_chat_ids, pinned_chat_ids, exclude_chat_ids. Peer lists REPLACE that list; [] clears it. Use bare numeric chat ids; null clears optional display/flag fields. Save get_folder_snapshot first; optional expected_revision rejects a stale snapshot.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder_id": {"type": "integer", "description": "Private folder ID (from list_folders)"},
                    "patch": {"type": "object", "description": "Fields to patch"},
                    "expected_revision": {"type": "string", "description": "Optional revision from get_folder_snapshot"}
                },
                "required": ["folder_id", "patch"]
            }),
        },
        ToolDefinition {
            name: "list_folders".into(),
            description: "Get all dialog folders (filters) with their IDs, names, and emoji. Returns a list of folders usable with the other folder tools.".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "get_folder".into(),
            description: "Get detailed information about a specific folder including all included/excluded/pinned chats.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder_id": {"type": "integer", "description": "Folder ID (from list_folders)"}
                },
                "required": ["folder_id"]
            }),
        },
        ToolDefinition {
            name: "create_folder".into(),
            description: "Create a new dialog folder. chat_ids accepts bare numeric chat ids or @usernames.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Folder name (required, max 12 UTF-16 units)"},
                    "emoticon": {"type": "string", "description": "Folder emoji (optional)"},
                    "chat_ids": {"type": "array", "items": {"type": ["integer", "string"]}, "description": "Chat IDs or @usernames to include"},
                    "contacts": {"type": "boolean", "default": false},
                    "non_contacts": {"type": "boolean", "default": false},
                    "groups": {"type": "boolean", "default": false},
                    "broadcasts": {"type": "boolean", "default": false},
                    "bots": {"type": "boolean", "default": false},
                    "exclude_muted": {"type": "boolean", "default": false},
                    "exclude_read": {"type": "boolean", "default": false},
                    "exclude_archived": {"type": "boolean", "default": true}
                },
                "required": ["title"]
            }),
        },
        ToolDefinition {
            name: "add_chat_to_folder".into(),
            description: "Add a chat (bare numeric id or @username) to an existing folder. Idempotent.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder_id": {"type": "integer", "description": "Folder ID (from list_folders)"},
                    "chat_id": {"description": "Chat ID or @username to add"},
                    "pinned": {"type": "boolean", "description": "Pin the chat in this folder (default false)", "default": false}
                },
                "required": ["folder_id", "chat_id"]
            }),
        },
        ToolDefinition {
            name: "remove_chat_from_folder".into(),
            description: "Remove a chat (bare numeric id or @username) from a folder. Idempotent.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder_id": {"type": "integer", "description": "Folder ID (from list_folders)"},
                    "chat_id": {"description": "Chat ID or @username to remove"}
                },
                "required": ["folder_id", "chat_id"]
            }),
        },
        ToolDefinition {
            name: "delete_folder".into(),
            description: "Delete a folder. Chats in the folder are preserved, only the folder is removed.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder_id": {"type": "integer", "description": "Folder ID to delete (from list_folders)"}
                },
                "required": ["folder_id"]
            }),
        },
        ToolDefinition {
            name: "reorder_folders".into(),
            description: "Change the order of folders. folder_ids must list ALL existing folder IDs in the desired order.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder_ids": {"type": "array", "items": {"type": "integer"}, "description": "All folder IDs in the desired order"}
                },
                "required": ["folder_ids"]
            }),
        },
    ]
}

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<Option<CallToolResult>> {
    let text = match tool_name {
        "get_folder_limits" => get_folder_limits(client).await?,
        "get_folder_snapshot" => get_folder_snapshot(client).await?,
        "update_folder" => update_folder(client, args).await?,
        "list_folders" => list_folders(client).await?,
        "get_folder" => get_folder(client, args).await?,
        "create_folder" => create_folder(client, args).await?,
        "add_chat_to_folder" => add_chat_to_folder(client, args).await?,
        "remove_chat_from_folder" => remove_chat_from_folder(client, args).await?,
        "delete_folder" => delete_folder(client, args).await?,
        "reorder_folders" => reorder_folders(client, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(CallToolResult::ok(text)))
}
