//! Contacts MCP tools.
//!
//! Ported from the Python `telegram_mcp` `tools/contacts.py`.
//!
//! Porting notes:
//! - Raw API calls go through `client.invoke(&tl::functions::contacts::…)`.
//! - Contact aliases (`set_contact_alias`, `list_contact_aliases`,
//!   `delete_contact_alias`) are a local-only feature invented by the Python
//!   version (persisted to a local file); grammers has no corresponding
//!   Telegram API, so these tools return `Err("not supported")`.

use super::str_arg;
use crate::mcp::{CallToolResult, ToolDefinition};
use anyhow::{Context, Result, bail};
use grammers_client::Client;
use grammers_client::peer::Peer;
use grammers_session::types::{PeerAuth, PeerId, PeerRef};
use grammers_tl_types as tl;
use serde_json::{Value, json};

fn tool(name: &str, description: &str, input_schema: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: description.into(),
        input_schema,
    }
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        tool(
            "list_contacts",
            "List all contacts in your Telegram account.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "search_contacts",
            "Search for contacts by name, username, or phone number using contacts.Search.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search term to look for in contact names, usernames, or phone numbers." }
                },
                "required": ["query"]
            }),
        ),
        tool(
            "get_contact_ids",
            "Get all contact IDs in your Telegram account.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "get_direct_chat_by_contact",
            "Find a direct chat with a specific contact by name, username, or phone.",
            json!({
                "type": "object",
                "properties": {
                    "contact_query": { "type": "string", "description": "Name, username, or phone number to search for." }
                },
                "required": ["contact_query"]
            }),
        ),
        tool(
            "get_contact_chats",
            "List all chats involving a specific contact (direct chat plus common groups/channels).",
            json!({
                "type": "object",
                "properties": {
                    "contact_id": { "description": "The numeric user ID or username of the contact." }
                },
                "required": ["contact_id"]
            }),
        ),
        tool(
            "get_last_interaction",
            "Get the most recent messages with a contact.",
            json!({
                "type": "object",
                "properties": {
                    "contact_id": { "description": "The numeric user ID or username of the contact." },
                    "limit": { "type": "integer", "description": "Max number of messages, newest first (default: 5)", "default": 5 }
                },
                "required": ["contact_id"]
            }),
        ),
        tool(
            "add_contact",
            "Add a new contact to your Telegram account. Either phone or username must be provided.",
            json!({
                "type": "object",
                "properties": {
                    "phone": { "type": "string", "description": "Phone number with country code. Required if username is not provided." },
                    "username": { "type": "string", "description": "Telegram username (without @). Use for adding contacts without phone numbers." },
                    "first_name": { "type": "string", "description": "The contact's first name." },
                    "last_name": { "type": "string", "description": "The contact's last name (optional)." }
                }
            }),
        ),
        tool(
            "delete_contact",
            "Delete a contact by user ID or username.",
            json!({
                "type": "object",
                "properties": {
                    "user_id": { "description": "The Telegram user ID or username of the contact to delete." }
                },
                "required": ["user_id"]
            }),
        ),
        tool(
            "block_user",
            "Block a user by user ID or username.",
            json!({
                "type": "object",
                "properties": {
                    "user_id": { "description": "The Telegram user ID or username to block." }
                },
                "required": ["user_id"]
            }),
        ),
        tool(
            "unblock_user",
            "Unblock a user by user ID or username.",
            json!({
                "type": "object",
                "properties": {
                    "user_id": { "description": "The Telegram user ID or username to unblock." }
                },
                "required": ["user_id"]
            }),
        ),
        tool(
            "import_contacts",
            "Import a list of contacts. Each contact is an object with phone, first_name, last_name.",
            json!({
                "type": "object",
                "properties": {
                    "contacts": {
                        "type": "array",
                        "description": "Contacts to import.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "phone": { "type": "string" },
                                "first_name": { "type": "string" },
                                "last_name": { "type": "string" }
                            },
                            "required": ["phone", "first_name"]
                        }
                    }
                },
                "required": ["contacts"]
            }),
        ),
        tool(
            "export_contacts",
            "Export all contacts as a JSON string.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "get_blocked_users",
            "Get a list of blocked users.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "send_contact",
            "Send a contact card to a chat.",
            json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "integer", "description": "The chat ID." },
                    "phone_number": { "type": "string", "description": "Contact's phone number." },
                    "first_name": { "type": "string", "description": "Contact's first name." },
                    "last_name": { "type": "string", "description": "Contact's last name (optional)." },
                    "vcard": { "type": "string", "description": "Additional vCard data (optional)." }
                },
                "required": ["chat_id", "phone_number", "first_name"]
            }),
        ),
        tool(
            "set_contact_alias",
            "Remember what the user calls someone (local-only alias). Not supported in this build.",
            json!({
                "type": "object",
                "properties": {
                    "alias": { "type": "string", "description": "The free-text reference to remember." },
                    "chat_id": { "type": "string", "description": "Chat ID, username (@user), or phone of the target." },
                    "replace": { "type": "boolean", "description": "Required to repoint an alias that already points at someone else (default: false)", "default": false }
                },
                "required": ["alias", "chat_id"]
            }),
        ),
        tool(
            "list_contact_aliases",
            "List remembered contact aliases (local-only). Not supported in this build.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "delete_contact_alias",
            "Forget one remembered alias (local-only). Not supported in this build.",
            json!({
                "type": "object",
                "properties": {
                    "alias": { "type": "string", "description": "The exact alias to delete." }
                },
                "required": ["alias"]
            }),
        ),
    ]
}

// ---------- helpers ----------

fn full_name(user: &tl::types::User) -> String {
    format!(
        "{} {}",
        user.first_name.as_deref().unwrap_or(""),
        user.last_name.as_deref().unwrap_or("")
    )
    .trim()
    .to_string()
}

/// Format one Telegram user into a compact JSON record.
fn user_record(user: &tl::types::User) -> Value {
    let mut record = serde_json::Map::new();
    record.insert("id".into(), json!(user.id));
    record.insert("name".into(), json!(full_name(user)));
    if let Some(username) = user.username.as_deref().filter(|s| !s.is_empty()) {
        record.insert("username".into(), json!(username));
    }
    if let Some(phone) = user.phone.as_deref().filter(|s| !s.is_empty()) {
        record.insert("phone".into(), json!(phone));
    }
    Value::Object(record)
}

fn tl_users(users: Vec<tl::enums::User>) -> Vec<tl::types::User> {
    users
        .into_iter()
        .filter_map(|u| match u {
            tl::enums::User::User(u) => Some(u),
            _ => None,
        })
        .collect()
}

/// Fetch the account's contact list as full `types::User` entries.
async fn fetch_contacts(client: &Client) -> Result<Vec<tl::types::User>> {
    let response = client
        .invoke(&tl::functions::contacts::GetContacts { hash: 0 })
        .await
        .context("contacts.GetContacts failed")?;
    match response {
        tl::enums::contacts::Contacts::Contacts(c) => Ok(tl_users(c.users)),
        tl::enums::contacts::Contacts::NotModified => Ok(Vec::new()),
    }
}

/// Resolve a "contact_id"/"user_id" argument (numeric id or username) to
/// `(bare_id, PeerRef, display_name)`.
async fn resolve_peer(client: &Client, value: &Value) -> Result<(i64, PeerRef, String)> {
    let normalized;
    let value = match value.as_str() {
        Some(text) => {
            let name = text.strip_prefix('@').unwrap_or(text);
            match name.parse::<i64>() {
                Ok(id) => {
                    normalized = json!(id);
                    &normalized
                }
                Err(_) => return resolve_peer_by_username(client, text, name).await,
            }
        }
        None => value,
    };
    match value.as_i64() {
        Some(id) => resolve_peer_by_id(client, id).await,
        None => bail!("user_id must be a numeric id or username"),
    }
}

/// Resolve a numeric user id via the contact list (for its access hash).
async fn resolve_peer_by_id(client: &Client, id: i64) -> Result<(i64, PeerRef, String)> {
    let contacts = fetch_contacts(client).await?;
    let user = contacts
        .into_iter()
        .find(|u| u.id == id)
        .with_context(|| format!("No contact found with id {id}"))?;
    let name = full_name(&user);
    let access_hash = user
        .access_hash
        .with_context(|| format!("No access hash available for contact {id}"))?;
    let peer_ref = PeerRef {
        id: PeerId::user(id),
        auth: PeerAuth::from_hash(access_hash),
    };
    Ok((id, peer_ref, name))
}

/// Resolve a username via `resolve_username`.
async fn resolve_peer_by_username(
    client: &Client,
    text: &str,
    name: &str,
) -> Result<(i64, PeerRef, String)> {
    let peer = client
        .resolve_username(name)
        .await
        .context("Failed to resolve username")?
        .with_context(|| format!("No user found for '{text}'"))?;
    let id = match &peer {
        Peer::User(u) => u.id().bare_id(),
        _ => bail!("'{text}' is not a user"),
    };
    let display = peer.name().map(str::to_string).unwrap_or_default();
    let peer_ref = peer.to_ref().await.context("Failed to get peer ref")?;
    Ok((id, peer_ref, display))
}

/// Map user ids to the direct-chat (DM) peer refs found in the dialog list.
async fn direct_chats(client: &Client) -> Result<std::collections::HashMap<i64, PeerRef>> {
    let mut map = std::collections::HashMap::new();
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.context("Failed to fetch dialogs")? {
        if let Peer::User(u) = dialog.peer() {
            map.entry(u.id().bare_id())
                .or_insert_with(|| dialog.peer_ref());
        }
    }
    Ok(map)
}

fn chat_info(chat: &tl::enums::Chat) -> (i64, String, String) {
    let id = chat.id();
    let (title, kind) = match chat {
        tl::enums::Chat::Chat(c) => (c.title.clone(), "group"),
        tl::enums::Chat::Channel(c) => {
            let kind = if c.broadcast { "channel" } else { "group" };
            (c.title.clone(), kind)
        }
        tl::enums::Chat::Forbidden(c) => (c.title.clone(), "group"),
        tl::enums::Chat::ChannelForbidden(c) => (c.title.clone(), "channel"),
        tl::enums::Chat::Empty(_) => ("(unknown)".to_string(), "unknown"),
    };
    (id, title, kind.to_string())
}

// ---------- handlers ----------

pub async fn try_handle(
    client: &Client,
    tool_name: &str,
    args: &Value,
) -> Result<Option<CallToolResult>> {
    let text = match tool_name {
        "list_contacts" => {
            let contacts = fetch_contacts(client).await?;
            if contacts.is_empty() {
                "No contacts found.".to_string()
            } else {
                serde_json::to_string_pretty(
                    &contacts.iter().map(user_record).collect::<Vec<_>>(),
                )?
            }
        }

        "search_contacts" => {
            let query = str_arg(args, "query");
            let response = client
                .invoke(&tl::functions::contacts::Search {
                    q: query.to_string(),
                    limit: 50,
                })
                .await
                .context("contacts.Search failed")?;
            let tl::enums::contacts::Found::Found(found) = response;
            let records: Vec<Value> = tl_users(found.users).iter().map(user_record).collect();
            if records.is_empty() {
                format!("No contacts found matching '{query}'.")
            } else {
                serde_json::to_string_pretty(&records)?
            }
        }

        "get_contact_ids" => {
            let ids = client
                .invoke(&tl::functions::contacts::GetContactIds { hash: 0 })
                .await
                .context("contacts.GetContactIDs failed")?;
            if ids.is_empty() {
                "No contact IDs found.".to_string()
            } else {
                format!(
                    "Contact IDs: {}",
                    ids.iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }

        "get_direct_chat_by_contact" => {
            let query = str_arg(args, "contact_query");
            let q = query.to_lowercase();
            let contacts = fetch_contacts(client).await?;
            let matched: Vec<&tl::types::User> = contacts
                .iter()
                .filter(|u| {
                    full_name(u).to_lowercase().contains(&q)
                        || u.username
                            .as_deref()
                            .map(|s| s.to_lowercase().contains(&q))
                            .unwrap_or(false)
                        || u.phone.as_deref().map(|s| s.contains(query)).unwrap_or(false)
                })
                .collect();
            if matched.is_empty() {
                format!("No contacts found matching '{query}'.")
            } else {
                let dms = direct_chats(client).await?;
                let mut records = Vec::new();
                for contact in &matched {
                    if dms.contains_key(&contact.id) {
                        let mut record = serde_json::Map::new();
                        record.insert("chat_id".into(), json!(contact.id));
                        record.insert("contact".into(), json!(full_name(contact)));
                        if let Some(username) =
                            contact.username.as_deref().filter(|s| !s.is_empty())
                        {
                            record.insert("username".into(), json!(username));
                        }
                        records.push(Value::Object(record));
                    }
                }
                if records.is_empty() {
                    let names: Vec<String> = matched.iter().map(|u| full_name(u)).collect();
                    format!(
                        "Found contacts: {}, but no direct chats were found with them.",
                        names.join(", ")
                    )
                } else {
                    serde_json::to_string_pretty(&records)?
                }
            }
        }

        "get_contact_chats" => {
            let value = args.get("contact_id").unwrap_or(&Value::Null);
            let (contact_id, peer_ref, contact_name) = resolve_peer(client, value).await?;
            let mut records = Vec::new();

            let dms = direct_chats(client).await?;
            if let Some(dm) = dms.get(&contact_id) {
                records.push(json!({
                    "chat_id": i64::from(*dm),
                    "type": "Private",
                }));
            }

            if let Ok(common) = client
                .invoke(&tl::functions::messages::GetCommonChats {
                    user_id: tl::enums::InputUser::from(peer_ref),
                    max_id: 0,
                    limit: 100,
                })
                .await
            {
                let chats = match common {
                    tl::enums::messages::Chats::Chats(c) => c.chats,
                    tl::enums::messages::Chats::Slice(c) => c.chats,
                };
                for chat in &chats {
                    let (id, title, kind) = chat_info(chat);
                    records.push(json!({
                        "chat_id": id,
                        "title": title,
                        "type": kind,
                    }));
                }
            }

            if records.is_empty() {
                format!("No chats found with {contact_name} (ID: {contact_id}).")
            } else {
                serde_json::to_string_pretty(&json!({
                    "contact_name": contact_name,
                    "contact_id": contact_id,
                    "chats": records,
                }))?
            }
        }

        "get_last_interaction" => {
            let value = args.get("contact_id").unwrap_or(&Value::Null);
            let limit = super::usize_arg(args, "limit", 5);
            let (contact_id, peer_ref, contact_name) = resolve_peer(client, value).await?;

            let mut iter = client.iter_messages(peer_ref);
            let mut records = Vec::new();
            while let Some(msg) = iter.next().await.context("Failed to fetch messages")? {
                records.push(json!({
                    "date": msg.date().format("%Y-%m-%d %H:%M:%S").to_string(),
                    "from": if msg.outgoing() { "You".to_string() } else { contact_name.clone() },
                    "text": msg.text(),
                }));
                if records.len() >= limit {
                    break;
                }
            }

            if records.is_empty() {
                format!("No messages found with {contact_name} (ID: {contact_id}).")
            } else {
                serde_json::to_string_pretty(&json!({
                    "contact_name": contact_name,
                    "contact_id": contact_id,
                    "messages": records,
                }))?
            }
        }

        "add_contact" => {
            let phone = str_arg(args, "phone");
            let username = str_arg(args, "username").trim_start_matches('@');
            let first_name = str_arg(args, "first_name");
            let last_name = str_arg(args, "last_name");

            if username.is_empty() && phone.is_empty() {
                return Err(anyhow::anyhow!(
                    "Either phone or username must be provided."
                ));
            }

            if !username.is_empty() {
                let peer = client
                    .resolve_username(username)
                    .await
                    .context("Failed to resolve username")?
                    .with_context(|| format!("No user found for @{username}"))?;
                let input: tl::enums::InputUser = match &peer {
                    Peer::User(_) => peer
                        .to_ref()
                        .await
                        .context("Failed to get peer ref")?
                        .into(),
                    _ => bail!("@{username} is not a user"),
                };
                client
                    .invoke(&tl::functions::contacts::AddContact {
                        add_phone_privacy_exception: false,
                        id: input,
                        first_name: first_name.to_string(),
                        last_name: last_name.to_string(),
                        phone: String::new(),
                        note: None,
                    })
                    .await
                    .context("contacts.AddContact failed")?;
                format!("Contact {first_name} {last_name} (@{username}) added successfully.")
            } else {
                let response = client
                    .invoke(&tl::functions::contacts::ImportContacts {
                        contacts: vec![tl::enums::InputContact::InputPhoneContact(
                            tl::types::InputPhoneContact {
                                client_id: 0,
                                phone: phone.to_string(),
                                first_name: first_name.to_string(),
                                last_name: last_name.to_string(),
                                note: None,
                            },
                        )],
                    })
                    .await
                    .context("contacts.ImportContacts failed")?;
                let tl::enums::contacts::ImportedContacts::Contacts(imported) = response;
                if imported.imported.is_empty() {
                    format!(
                        "Contact {first_name} {last_name} was not added (no imported users returned)."
                    )
                } else {
                    format!("Contact {first_name} {last_name} added successfully.")
                }
            }
        }

        "delete_contact" => {
            let value = args.get("user_id").unwrap_or(&Value::Null);
            let (contact_id, peer_ref, _) = resolve_peer(client, value).await?;
            client
                .invoke(&tl::functions::contacts::DeleteContacts {
                    id: vec![tl::enums::InputUser::from(peer_ref)],
                })
                .await
                .context("contacts.DeleteContacts failed")?;
            format!("Contact with user ID {contact_id} deleted.")
        }

        "block_user" => {
            let value = args.get("user_id").unwrap_or(&Value::Null);
            let (contact_id, peer_ref, _) = resolve_peer(client, value).await?;
            let blocked = client
                .invoke(&tl::functions::contacts::Block {
                    my_stories_from: false,
                    id: tl::enums::InputPeer::from(peer_ref),
                })
                .await
                .context("contacts.Block failed")?;
            if blocked {
                format!("User {contact_id} blocked.")
            } else {
                format!("User {contact_id} block request returned false.")
            }
        }

        "unblock_user" => {
            let value = args.get("user_id").unwrap_or(&Value::Null);
            let (contact_id, peer_ref, _) = resolve_peer(client, value).await?;
            let unblocked = client
                .invoke(&tl::functions::contacts::Unblock {
                    my_stories_from: false,
                    id: tl::enums::InputPeer::from(peer_ref),
                })
                .await
                .context("contacts.Unblock failed")?;
            if unblocked {
                format!("User {contact_id} unblocked.")
            } else {
                format!("User {contact_id} unblock request returned false.")
            }
        }

        "import_contacts" => {
            let list = args
                .get("contacts")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if list.is_empty() {
                return Err(anyhow::anyhow!("contacts must be a non-empty array."));
            }
            let input: Vec<tl::enums::InputContact> = list
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    tl::enums::InputContact::InputPhoneContact(tl::types::InputPhoneContact {
                        client_id: i as i64,
                        phone: c.get("phone").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        first_name: c
                            .get("first_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        last_name: c
                            .get("last_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        note: None,
                    })
                })
                .collect();
            let response = client
                .invoke(&tl::functions::contacts::ImportContacts { contacts: input })
                .await
                .context("contacts.ImportContacts failed")?;
            let tl::enums::contacts::ImportedContacts::Contacts(imported) = response;
            format!("Imported {} contacts.", imported.imported.len())
        }

        "export_contacts" => {
            let contacts = fetch_contacts(client).await?;
            serde_json::to_string_pretty(&contacts.iter().map(user_record).collect::<Vec<_>>())?
        }

        "get_blocked_users" => {
            let response = client
                .invoke(&tl::functions::contacts::GetBlocked {
                    my_stories_from: false,
                    offset: 0,
                    limit: 100,
                })
                .await
                .context("contacts.GetBlocked failed")?;
            let users = match response {
                tl::enums::contacts::Blocked::Blocked(b) => b.users,
                tl::enums::contacts::Blocked::Slice(s) => s.users,
            };
            let records: Vec<Value> = tl_users(users).iter().map(user_record).collect();
            serde_json::to_string_pretty(&records)?
        }

        "send_contact" => {
            let chat_id = super::i64_arg(args, "chat_id", 0);
            let phone_number = str_arg(args, "phone_number");
            let first_name = str_arg(args, "first_name");
            let last_name = str_arg(args, "last_name");
            let vcard = str_arg(args, "vcard");

            let dms = direct_chats(client).await?;
            let mut target: Option<PeerRef> = dms.get(&chat_id).copied();
            if target.is_none() {
                let mut dialogs = client.iter_dialogs();
                while let Some(dialog) = dialogs.next().await.context("Failed to fetch dialogs")? {
                    if dialog.peer().id().bare_id() == chat_id {
                        target = Some(dialog.peer_ref());
                        break;
                    }
                }
            }
            let peer_ref = target.with_context(|| {
                format!("No chat found with id {chat_id}. Use list_chats to find IDs.")
            })?;

            let random_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0);
            client
                .invoke(&tl::functions::messages::SendMedia {
                    silent: false,
                    background: false,
                    clear_draft: false,
                    noforwards: false,
                    update_stickersets_order: false,
                    invert_media: false,
                    allow_paid_floodskip: false,
                    peer: tl::enums::InputPeer::from(peer_ref),
                    reply_to: None,
                    media: tl::enums::InputMedia::Contact(tl::types::InputMediaContact {
                        phone_number: phone_number.to_string(),
                        first_name: first_name.to_string(),
                        last_name: last_name.to_string(),
                        vcard: vcard.to_string(),
                    }),
                    message: String::new(),
                    random_id,
                    reply_markup: None,
                    entities: None,
                    schedule_date: None,
                    schedule_repeat_period: None,
                    send_as: None,
                    quick_reply_shortcut: None,
                    effect: None,
                    allow_paid_stars: None,
                    suggested_post: None,
                })
                .await
                .context("messages.SendMedia failed")?;
            format!("Contact sent to chat {chat_id}.")
        }

        // Contact aliases are a local-only feature invented by the Python
        // version; grammers has no corresponding Telegram API.
        // TODO: implement local alias storage (persisted file) when the
        // alias feature is re-introduced on the Rust side.
        "set_contact_alias" => {
            bail!("not supported: local alias feature")
        }
        "list_contact_aliases" => {
            bail!("not supported: local alias feature")
        }
        "delete_contact_alias" => {
            bail!("not supported: local alias feature")
        }

        _ => return Ok(None),
    };

    Ok(Some(CallToolResult::ok(text)))
}
