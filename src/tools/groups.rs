//! Group and channel administration tools.
//!
//! Faithful Rust port of `telegram_mcp/tools/groups.py`: create/invite/leave
//! groups and channels, edit title/photo/about, promote/demote/ban/unban/remove
//! members, manage default permissions and slow mode, read admins, banned
//! users, member roles, invite links and the admin log.
//!
//! Chat ids are bare numeric ids (as returned by `list_chats`) or `@username`
//! strings. User ids likewise.

use super::{bool_arg, i64_arg, str_arg, usize_arg};
use crate::mcp::{CallToolResult, ToolDefinition};
use anyhow::{Context, bail};
use grammers_client::Client;
use grammers_session::types::{PeerKind, PeerRef};
use grammers_tl_types as tl;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// Human kind name for a resolved peer (used in error messages).
fn kind_name(pr: &PeerRef) -> &'static str {
    match pr.id.kind() {
        PeerKind::User => "user",
        PeerKind::Chat => "basic group",
        PeerKind::Channel => "channel/supergroup",
    }
}

/// Resolve a chat argument (bare numeric id or @username) to a PeerRef.
async fn resolve_peer(client: &Client, value: &Value) -> anyhow::Result<PeerRef> {
    if let Some(n) = value.as_i64() {
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await.context("Failed to iterate dialogs")? {
            if dialog.peer().id().bare_id() == Some(n) {
                return Ok(dialog.peer_ref());
            }
        }
        bail!("Could not resolve chat id {n}. Use list_chats to find IDs.");
    }
    if let Some(s) = value.as_str() {
        let username = s.strip_prefix('@').unwrap_or(s);
        let peer = client
            .resolve_username(username)
            .await
            .context("Failed to resolve username")?
            .with_context(|| format!("No user/channel found for @{username}"))?;
        return peer.to_ref().await.context("Failed to get peer ref");
    }
    bail!("Chat ID must be a numeric ID or a @username string")
}

/// Resolve and require a user peer (for membership-target arguments).
async fn resolve_user_peer(client: &Client, value: &Value) -> anyhow::Result<PeerRef> {
    let pr = resolve_peer(client, value).await?;
    if pr.id.kind() != PeerKind::User {
        bail!("Expected a user, but the argument resolved to a {}", kind_name(&pr));
    }
    Ok(pr)
}

/// Best-effort display name for a peer (dialog title or bare id).
async fn peer_display_name(client: &Client, pr: &PeerRef) -> String {
    let mut dialogs = client.iter_dialogs();
    while let Ok(Some(dialog)) = dialogs.next().await {
        if dialog.peer_ref().id == pr.id {
            return dialog.peer().name().unwrap_or("(no name)").to_string();
        }
    }
    format!("chat {}", pr.id.bare_id().unwrap_or(0))
}

/// Require a channel/supergroup peer and return its InputChannel.
fn require_channel(pr: &PeerRef) -> anyhow::Result<tl::enums::InputChannel> {
    let input = tl::enums::InputChannel::from(pr);
    if matches!(input, tl::enums::InputChannel::Channel(_)) {
        Ok(input)
    } else {
        bail!(
            "This operation requires a channel/supergroup, but the peer is a {}",
            kind_name(pr)
        )
    }
}

/// Convert a user PeerRef to InputUser.
fn input_user_of(pr: &PeerRef) -> anyhow::Result<tl::enums::InputUser> {
    match tl::enums::InputUser::from(pr) {
        tl::enums::InputUser::Empty => bail!("Peer is not a user"),
        u => Ok(u),
    }
}

/// Raw positive chat id for basic groups (0 for other kinds).
fn basic_chat_id(pr: &PeerRef) -> i64 {
    i64::from(pr)
}

/// Whether the channel peer is a megagroup (None = unknown, e.g. not in dialogs).
async fn channel_is_megagroup(client: &Client, pr: &PeerRef) -> Option<bool> {
    let mut dialogs = client.iter_dialogs();
    while let Ok(Some(dialog)) = dialogs.next().await {
        if dialog.peer_ref().id == pr.id
            && let grammers_client::peer::Peer::Channel(c) = dialog.peer()
        {
            return Some(c.is_megagroup());
        }
    }
    None
}

/// Pull the first chat id out of an Updates result (used by create_*).
fn first_chat_id_of_updates(updates: &tl::enums::Updates) -> Option<i64> {
    match updates {
        tl::enums::Updates::Updates(u) => u.chats.first().map(|c| c.id()),
        _ => None,
    }
}

/// Extract the hash part from a t.me invite link (or accept a bare hash).
fn invite_hash(link: &str) -> String {
    let part = link.rsplit('/').next().unwrap_or(link);
    part.strip_prefix('+').unwrap_or(part).to_string()
}

/// One-line "id | @username | name" rendering of a grammers User.
fn user_line(user: &grammers_client::peer::User) -> String {
    let id = user.id().bare_id().unwrap_or(0);
    let name = format!(
        "{} {}",
        user.first_name().unwrap_or(""),
        user.last_name().unwrap_or("")
    );
    let name = name.trim();
    let name = if name.is_empty() { "(no name)" } else { name };
    match user.username() {
        Some(u) => format!("- id:{id} | @{u} | {name}"),
        None => format!("- id:{id} | {name}"),
    }
}

/// Build ChatAdminRights from a JSON object of bool flags.
fn admin_rights_from(obj: Option<&Value>, default: bool) -> tl::types::ChatAdminRights {
    let get = |key: &str| {
        obj.and_then(|o| o.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    tl::types::ChatAdminRights {
        change_info: get("change_info"),
        post_messages: get("post_messages"),
        edit_messages: get("edit_messages"),
        delete_messages: get("delete_messages"),
        ban_users: get("ban_users"),
        invite_users: get("invite_users"),
        pin_messages: get("pin_messages"),
        add_admins: get("add_admins"),
        anonymous: get("anonymous"),
        manage_call: get("manage_call"),
        other: get("other"),
        manage_topics: get("manage_topics"),
        post_stories: false,
        edit_stories: false,
        delete_stories: false,
        manage_direct_messages: false,
    }
}

/// Build ChatAdminRights from individual boolean arguments.
#[allow(clippy::too_many_arguments)]
fn admin_rights_individual(
    change_info: bool,
    post_messages: bool,
    edit_messages: bool,
    delete_messages: bool,
    ban_users: bool,
    invite_users: bool,
    pin_messages: bool,
    add_admins: bool,
    anonymous: bool,
    manage_call: bool,
    manage_topics: bool,
    other: bool,
) -> tl::types::ChatAdminRights {
    tl::types::ChatAdminRights {
        change_info,
        post_messages,
        edit_messages,
        delete_messages,
        ban_users,
        invite_users,
        pin_messages,
        add_admins,
        anonymous,
        manage_call,
        other,
        manage_topics,
        post_stories: false,
        edit_stories: false,
        delete_stories: false,
        manage_direct_messages: false,
    }
}

/// Build ChatBannedRights; when `all` is true every restriction is enabled (ban),
/// otherwise everything is cleared (unban).
#[allow(clippy::too_many_arguments)]
fn banned_rights_all(all: bool) -> tl::types::ChatBannedRights {
    tl::types::ChatBannedRights {
        view_messages: all,
        send_messages: all,
        send_media: all,
        send_stickers: all,
        send_gifs: all,
        send_games: all,
        send_inline: all,
        embed_links: all,
        send_polls: all,
        change_info: all,
        invite_users: all,
        pin_messages: all,
        manage_topics: all,
        send_photos: all,
        send_videos: all,
        send_roundvideos: all,
        send_audios: all,
        send_voices: all,
        send_docs: all,
        send_plain: all,
        edit_rank: all,
        send_reactions: all,
        until_date: 0,
    }
}

/// Upload an image file and wrap it as an InputChatPhoto for chat photo edits.
async fn upload_chat_photo(
    client: &Client,
    file_path: &str,
) -> anyhow::Result<tl::enums::InputChatPhoto> {
    if !std::path::Path::new(file_path).is_file() {
        bail!("File not found: {file_path}");
    }
    let uploaded = client
        .upload_file(file_path)
        .await
        .map_err(|e| anyhow::anyhow!("File upload failed: {e}"))?;
    Ok(tl::enums::InputChatPhoto::InputChatUploadedPhoto(
        tl::types::InputChatUploadedPhoto {
            file: Some(uploaded.raw),
            video: None,
            video_start_ts: None,
            video_emoji_markup: None,
        },
    ))
}

/// Map a ChannelAdminLogEventAction variant to a short human name.
fn admin_log_action_name(action: &tl::enums::ChannelAdminLogEventAction) -> &'static str {
    use tl::enums::ChannelAdminLogEventAction as A;
    match action {
        A::ChangeTitle(_) => "change_title",
        A::ChangeAbout(_) => "change_about",
        A::ChangeUsername(_) => "change_username",
        A::ChangePhoto(_) => "change_photo",
        A::ToggleInvites(_) => "toggle_invites",
        A::ToggleSignatures(_) => "toggle_signatures",
        A::UpdatePinned(_) => "update_pinned",
        A::EditMessage(_) => "edit_message",
        A::DeleteMessage(_) => "delete_message",
        A::ParticipantJoin => "participant_join",
        A::ParticipantLeave => "participant_leave",
        A::ParticipantInvite(_) => "participant_invite",
        A::ParticipantToggleBan(_) => "participant_toggle_ban",
        A::ParticipantToggleAdmin(_) => "participant_toggle_admin",
        A::ChangeStickerSet(_) => "change_sticker_set",
        A::TogglePreHistoryHidden(_) => "toggle_pre_history_hidden",
        A::DefaultBannedRights(_) => "default_banned_rights",
        A::StopPoll(_) => "stop_poll",
        A::ChangeLinkedChat(_) => "change_linked_chat",
        A::ChangeLocation(_) => "change_location",
        A::ToggleSlowMode(_) => "toggle_slow_mode",
        A::StartGroupCall(_) => "start_group_call",
        A::DiscardGroupCall(_) => "discard_group_call",
        A::ParticipantMute(_) => "participant_mute",
        A::ParticipantUnmute(_) => "participant_unmute",
        A::ToggleGroupCallSetting(_) => "toggle_group_call_setting",
        A::ParticipantJoinByInvite(_) => "participant_join_by_invite",
        A::ExportedInviteDelete(_) => "exported_invite_delete",
        A::ExportedInviteRevoke(_) => "exported_invite_revoke",
        A::ExportedInviteEdit(_) => "exported_invite_edit",
        A::ParticipantVolume(_) => "participant_volume",
        A::ChangeHistoryTtl(_) => "change_history_ttl",
        A::ParticipantJoinByRequest(_) => "participant_join_by_request",
        A::ToggleNoForwards(_) => "toggle_no_forwards",
        A::SendMessage(_) => "send_message",
        A::ChangeAvailableReactions(_) => "change_available_reactions",
        A::ChangeUsernames(_) => "change_usernames",
        A::ToggleForum(_) => "toggle_forum",
        A::CreateTopic(_) => "create_topic",
        A::EditTopic(_) => "edit_topic",
        A::DeleteTopic(_) => "delete_topic",
        A::PinTopic(_) => "pin_topic",
        A::ToggleAntiSpam(_) => "toggle_anti_spam",
        A::ChangePeerColor(_) => "change_peer_color",
        A::ChangeProfilePeerColor(_) => "change_profile_peer_color",
        A::ChangeWallpaper(_) => "change_wallpaper",
        A::ChangeEmojiStatus(_) => "change_emoji_status",
        A::ChangeEmojiStickerSet(_) => "change_emoji_sticker_set",
        A::ToggleSignatureProfiles(_) => "toggle_signature_profiles",
        A::ParticipantSubExtend(_) => "participant_sub_extend",
        A::ToggleAutotranslation(_) => "toggle_autotranslation",
        A::ParticipantEditRank(_) => "participant_edit_rank",
    }
}

// ---------------------------------------------------------------------------
// tool implementations
// ---------------------------------------------------------------------------

/// Create a new (basic) group and add users.
async fn create_group(client: &Client, args: &Value) -> anyhow::Result<String> {
    let title = str_arg(args, "title");
    if title.is_empty() {
        bail!("title is required");
    }
    let user_ids = args
        .get("user_ids")
        .and_then(|v| v.as_array())
        .context("user_ids must be an array of user IDs or @usernames")?;
    let mut users = Vec::new();
    for uid in user_ids {
        match resolve_user_peer(client, uid).await {
            Ok(pr) => users.push(input_user_of(&pr)?),
            Err(_) => {
                return Ok("Error: Could not find a requested user.".to_string());
            }
        }
    }
    if users.is_empty() {
        return Ok("Error: No valid users provided".to_string());
    }
    let result = client
        .invoke(&tl::functions::messages::CreateChat {
            users,
            title: title.to_string(),
            ttl_period: None,
        })
        .await
        .context("messages.createChat failed")?;
    match result {
        tl::enums::messages::InvitedUsers::Users(u) => {
            let missing: Vec<String> = u
                .missing_invitees
                .iter()
                .map(|m| match m {
                    tl::enums::MissingInvitee::User(m) => m.user_id.to_string(),
                    tl::enums::MissingInvitee::Channel(m) => m.channel_id.to_string(),
                })
                .collect();
            let mut out = match first_chat_id_of_updates(&u.updates) {
                Some(id) => format!("Group '{title}' created with ID: {id}"),
                None => format!("Group '{title}' created successfully."),
            };
            if !missing.is_empty() {
                out.push_str(&format!(" (not added: {})", missing.join(", ")));
            }
            Ok(out)
        }
    }
}

/// Invite users to a group or channel.
async fn invite_to_group(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(
        client,
        args.get("group_id").context("group_id is required")?,
    )
    .await?;
    let chat_name = peer_display_name(client, &chat).await;
    let user_ids = args
        .get("user_ids")
        .and_then(|v| v.as_array())
        .context("user_ids must be an array of user IDs or @usernames")?;
    let mut users = Vec::new();
    for uid in user_ids {
        match resolve_user_peer(client, uid).await {
            Ok(pr) => users.push(input_user_of(&pr)?),
            Err(_) => return Ok("Error: A requested user could not be found.".to_string()),
        }
    }
    match chat.id.kind() {
        PeerKind::Channel => {
            let channel = require_channel(&chat)?;
            let result = client
                .invoke(&tl::functions::channels::InviteToChannel { channel, users })
                .await
                .context("channels.inviteToChannel failed")?;
            match result {
                tl::enums::messages::InvitedUsers::Users(u) => {
                    let missing = u.missing_invitees.len();
                    let invited = user_ids.len().saturating_sub(missing);
                    Ok(format!(
                        "Successfully invited {invited} user(s) to {chat_name} ({missing} not added due to privacy settings)."
                    ))
                }
            }
        }
        PeerKind::Chat => {
            let chat_id = basic_chat_id(&chat);
            let mut invited = 0usize;
            let mut already = 0usize;
            let mut failures: Vec<String> = Vec::new();
            for user in users {
                match client
                    .invoke(&tl::functions::messages::AddChatUser {
                        chat_id,
                        user_id: user,
                        fwd_limit: 100,
                    })
                    .await
                {
                    Ok(_) => invited += 1,
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("USER_ALREADY_PARTICIPANT") {
                            already += 1;
                        } else if msg.contains("USER_NOT_MUTUAL_CONTACT") {
                            failures.push("not mutual contact".to_string());
                        } else if msg.contains("USER_PRIVACY_RESTRICTED") {
                            failures.push("privacy restricted".to_string());
                        } else {
                            failures.push(msg);
                        }
                    }
                }
            }
            let mut out = format!("Successfully invited {invited} user(s) to {chat_name}");
            if already > 0 {
                out.push_str(&format!(" ({already} already a participant)"));
            }
            if !failures.is_empty() {
                out.push_str(&format!(" (failed: {})", failures.join("; ")));
            }
            Ok(out)
        }
        PeerKind::User => Ok("Error: cannot invite users to a user chat.".to_string()),
    }
}

/// Leave a group or channel.
async fn leave_chat(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let name = peer_display_name(client, &chat).await;
    match chat.id.kind() {
        PeerKind::Channel => {
            let channel = require_channel(&chat)?;
            client
                .invoke(&tl::functions::channels::LeaveChannel { channel })
                .await
                .context("channels.leaveChannel failed")?;
            Ok(format!("Left channel/supergroup {name}."))
        }
        PeerKind::Chat => {
            let chat_id = basic_chat_id(&chat);
            client
                .invoke(&tl::functions::messages::DeleteChatUser {
                    revoke_history: false,
                    chat_id,
                    user_id: tl::enums::InputUser::UserSelf,
                })
                .await
                .context("messages.deleteChatUser failed")?;
            Ok(format!("Left basic group {name}."))
        }
        PeerKind::User => Ok("Error: this tool is for groups and channels only.".to_string()),
    }
}

/// List participants in a group or channel with pagination.
async fn get_participants(client: &Client, args: &Value) -> anyhow::Result<String> {
    let page = usize_arg(args, "page", 1);
    let page_size = usize_arg(args, "page_size", 200);
    if page < 1 {
        return Ok("Error: page must be at least 1.".to_string());
    }
    if !(1..=1000).contains(&page_size) {
        return Ok("Error: page_size must be between 1 and 1000 participants per request.".to_string());
    }
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    if chat.id.kind() == PeerKind::User {
        return Ok("Error: chat_id must be a group or channel.".to_string());
    }
    let skip = (page - 1) * page_size;
    let mut iter = client.iter_participants(chat);
    let mut skipped = 0usize;
    let mut records = Vec::new();
    let mut has_more = false;
    while let Some(participant) = iter.next().await.context("Failed to fetch participants")? {
        if skipped < skip {
            skipped += 1;
            continue;
        }
        if records.len() == page_size {
            has_more = true;
            break;
        }
        records.push(user_line(&participant.user));
    }
    if records.is_empty() {
        return Ok("No participants found.".to_string());
    }
    let mut out = records.join("\n");
    out.push_str(&format!("\n\nPage {page} (showing {} participants)", records.len()));
    if has_more {
        out.push_str(&format!(" — more results available on page {}", page + 1));
    }
    Ok(out)
}

/// Create a new channel or supergroup.
async fn create_channel(client: &Client, args: &Value) -> anyhow::Result<String> {
    let title = str_arg(args, "title");
    if title.is_empty() {
        bail!("title is required");
    }
    let about = str_arg(args, "about");
    let megagroup = bool_arg(args, "megagroup", false);
    let result = client
        .invoke(&tl::functions::channels::CreateChannel {
            broadcast: !megagroup,
            megagroup,
            for_import: false,
            forum: false,
            title: title.to_string(),
            about: about.to_string(),
            geo_point: None,
            address: None,
            ttl_period: None,
        })
        .await
        .context("channels.createChannel failed")?;
    let kind = if megagroup { "supergroup" } else { "channel" };
    match first_chat_id_of_updates(&result) {
        Some(id) => Ok(format!("{kind} '{title}' created with ID: {id}")),
        None => Ok(format!("{kind} '{title}' created successfully.")),
    }
}

/// Edit the title of a group or channel.
async fn edit_chat_title(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let title = str_arg(args, "title");
    if title.is_empty() {
        bail!("title is required");
    }
    match chat.id.kind() {
        PeerKind::Channel => {
            let channel = require_channel(&chat)?;
            client
                .invoke(&tl::functions::channels::EditTitle {
                    channel,
                    title: title.to_string(),
                })
                .await
                .context("channels.editTitle failed")?;
        }
        PeerKind::Chat => {
            let chat_id = basic_chat_id(&chat);
            client
                .invoke(&tl::functions::messages::EditChatTitle {
                    chat_id,
                    title: title.to_string(),
                })
                .await
                .context("messages.editChatTitle failed")?;
        }
        PeerKind::User => return Ok("Error: cannot edit the title of a user chat.".to_string()),
    }
    Ok(format!("Chat title updated to '{title}'."))
}

/// Edit the photo of a group or channel from an image file.
async fn edit_chat_photo(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let file_path = str_arg(args, "file_path");
    if file_path.is_empty() {
        bail!("file_path is required");
    }
    let photo = match upload_chat_photo(client, file_path).await {
        Ok(p) => p,
        Err(e) => return Ok(format!("Error: {e}")),
    };
    match chat.id.kind() {
        PeerKind::Channel => {
            let channel = require_channel(&chat)?;
            client
                .invoke(&tl::functions::channels::EditPhoto { channel, photo })
                .await
                .context("channels.editPhoto failed")?;
        }
        PeerKind::Chat => {
            let chat_id = basic_chat_id(&chat);
            client
                .invoke(&tl::functions::messages::EditChatPhoto { chat_id, photo })
                .await
                .context("messages.editChatPhoto failed")?;
        }
        PeerKind::User => return Ok("Error: cannot edit the photo of a user chat.".to_string()),
    }
    Ok("Chat photo updated.".to_string())
}

/// Edit the description ("About") of a group or channel.
async fn edit_chat_about(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let about = str_arg(args, "about");
    if about.chars().count() > 255 {
        return Ok("Error: description exceeds Telegram's 255 character limit.".to_string());
    }
    let result = client
        .invoke(&tl::functions::messages::EditChatAbout {
            peer: tl::enums::InputPeer::from(&chat),
            about: about.to_string(),
        })
        .await;
    match result {
        Ok(_) => Ok("Chat description updated.".to_string()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("CHAT_ABOUT_NOT_MODIFIED") {
                Ok("Chat description is already set to the requested value.".to_string())
            } else if msg.contains("CHAT_ADMIN_REQUIRED") {
                Ok("Error: admin rights required to edit the chat description.".to_string())
            } else {
                Err(anyhow::anyhow!("messages.editChatAbout failed: {msg}"))
            }
        }
    }
}

/// Delete the photo of a group or channel.
async fn delete_chat_photo(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let photo = tl::enums::InputChatPhoto::Empty;
    match chat.id.kind() {
        PeerKind::Channel => {
            let channel = require_channel(&chat)?;
            client
                .invoke(&tl::functions::channels::EditPhoto { channel, photo })
                .await
                .context("channels.editPhoto failed")?;
        }
        PeerKind::Chat => {
            let chat_id = basic_chat_id(&chat);
            client
                .invoke(&tl::functions::messages::EditChatPhoto { chat_id, photo })
                .await
                .context("messages.editChatPhoto failed")?;
        }
        PeerKind::User => return Ok("Error: cannot delete the photo of a user chat.".to_string()),
    }
    Ok("Chat photo deleted.".to_string())
}

/// Promote a user to admin with a default (broad) rights set.
async fn promote_admin(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(
        client,
        args.get("group_id").context("group_id is required")?,
    )
    .await?;
    let channel = require_channel(&chat)?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    let rank = str_arg(args, "rank");
    let rank = if rank.is_empty() { "Admin" } else { rank };
    let rights_obj = args.get("rights");
    // Python defaults: everything on except add_admins and anonymous.
    let mut rights = admin_rights_from(rights_obj, true);
    rights.add_admins = rights_obj
        .and_then(|o| o.get("add_admins"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    rights.anonymous = rights_obj
        .and_then(|o| o.get("anonymous"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    client
        .invoke(&tl::functions::channels::EditAdmin {
            channel,
            user_id: input_user_of(&user)?,
            admin_rights: tl::enums::ChatAdminRights::Rights(rights),
            rank: Some(rank.to_string()),
        })
        .await
        .context("channels.editAdmin failed")?;
    Ok(format!("User promoted to admin (rank '{rank}')."))
}

/// Demote a user from admin (clear all admin rights).
async fn demote_admin(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(
        client,
        args.get("group_id").context("group_id is required")?,
    )
    .await?;
    let channel = require_channel(&chat)?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    client
        .invoke(&tl::functions::channels::EditAdmin {
            channel,
            user_id: input_user_of(&user)?,
            admin_rights: tl::enums::ChatAdminRights::Rights(admin_rights_from(None, false)),
            rank: Some(String::new()),
        })
        .await
        .context("channels.editAdmin failed")?;
    Ok("User demoted from admin.".to_string())
}

/// Ban a user from a group or channel (permanent ban).
async fn ban_user(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let channel = require_channel(&chat)?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    let name = peer_display_name(client, &chat).await;
    client
        .invoke(&tl::functions::channels::EditBanned {
            channel,
            participant: tl::enums::InputPeer::from(&user),
            banned_rights: tl::enums::ChatBannedRights::Rights(banned_rights_all(true)),
        })
        .await
        .context("channels.editBanned failed")?;
    Ok(format!("User banned from {name}."))
}

/// Unban a user from a group or channel (clear all restrictions).
async fn unban_user(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let channel = require_channel(&chat)?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    let name = peer_display_name(client, &chat).await;
    client
        .invoke(&tl::functions::channels::EditBanned {
            channel,
            participant: tl::enums::InputPeer::from(&user),
            banned_rights: tl::enums::ChatBannedRights::Rights(banned_rights_all(false)),
        })
        .await
        .context("channels.editBanned failed")?;
    Ok(format!("User unbanned from {name}."))
}

/// Remove a user from a group or channel WITHOUT banning them.
async fn remove_user(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    let name = peer_display_name(client, &chat).await;
    if user.id.bare_id() == Some(client.get_me().await?.id().bare_id()) {
        return Ok("Error: remove_user cannot target the current account. Use leave_chat instead."
            .to_string());
    }
    match chat.id.kind() {
        PeerKind::Chat => {
            let chat_id = basic_chat_id(&chat);
            match client
                .invoke(&tl::functions::messages::DeleteChatUser {
                    revoke_history: false,
                    chat_id,
                    user_id: input_user_of(&user)?,
                })
                .await
            {
                Ok(_) => Ok(format!("User removed from {name}. No ban left in place.")),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("USER_NOT_PARTICIPANT") {
                        Ok("Error: The user is not a member of this chat.".to_string())
                    } else if msg.contains("CHAT_ADMIN_REQUIRED") {
                        Ok("Error: admin rights required to remove members from this chat."
                            .to_string())
                    } else if msg.contains("USER_ADMIN_INVALID") {
                        Ok("Error: Cannot remove this user - they are an admin. Demote them first (demote_admin).".to_string())
                    } else {
                        Err(anyhow::anyhow!("messages.deleteChatUser failed: {msg}"))
                    }
                }
            }
        }
        PeerKind::Channel => {
            let channel = require_channel(&chat)?;
            // Membership check first: editBanned on a non-member would silently
            // succeed (that's how pre-emptive bans work).
            let found = client
                .invoke(&tl::functions::channels::GetParticipant {
                    channel: require_channel(&chat)?,
                    participant: tl::enums::InputPeer::from(&user),
                })
                .await
                .context("channels.getParticipant failed")?;
            let is_member = match found {
                tl::enums::channels::ChannelParticipant::Participant(p) => {
                    !matches!(
                        p.participant,
                        tl::enums::ChannelParticipant::Left(_)
                            | tl::enums::ChannelParticipant::Banned(_)
                    )
                }
            };
            if !is_member {
                return Ok("Error: The user is not a member of this chat.".to_string());
            }
            // Eject: ban with view_messages, then clear the ban again so the
            // user is not left on the banned list.
            client
                .invoke(&tl::functions::channels::EditBanned {
                    channel: require_channel(&chat)?,
                    participant: tl::enums::InputPeer::from(&user),
                    banned_rights: tl::enums::ChatBannedRights::Rights({
                        let mut r = banned_rights_all(false);
                        r.view_messages = true;
                        r
                    }),
                })
                .await
                .context("eject (ban) step of remove_user failed")?;
            match client
                .invoke(&tl::functions::channels::EditBanned {
                    channel,
                    participant: tl::enums::InputPeer::from(&user),
                    banned_rights: tl::enums::ChatBannedRights::Rights(banned_rights_all(false)),
                })
                .await
            {
                Ok(_) => Ok(format!("User removed from {name}. No ban left in place.")),
                Err(e) => Ok(format!(
                    "Error: The user was ejected, but clearing the ban afterwards failed ({e}), so they are currently BANNED from this chat. Call unban_user to lift the ban."
                )),
            }
        }
        PeerKind::User => Ok("Error: chat_id must be a group or channel, not a user.".to_string()),
    }
}

/// Set default member permissions for a group/supergroup/channel.
#[allow(clippy::too_many_arguments)]
async fn set_default_chat_permissions(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    // Telegram semantics are inverted (True = banned), the tool args say what is allowed.
    let banned = |key: &str, allowed_default: bool| -> bool {
        !bool_arg(args, key, allowed_default)
    };
    let until_date = i64_arg(args, "until_date", 0) as i32;
    client
        .invoke(&tl::functions::messages::EditChatDefaultBannedRights {
            peer: tl::enums::InputPeer::from(&chat),
            banned_rights: tl::enums::ChatBannedRights::Rights(tl::types::ChatBannedRights {
                view_messages: false,
                send_messages: banned("send_messages", true),
                send_media: banned("send_media", true),
                send_stickers: banned("send_stickers", true),
                send_gifs: banned("send_gifs", true),
                send_games: banned("send_games", true),
                send_inline: banned("send_inline", true),
                embed_links: banned("embed_links", true),
                send_polls: banned("send_polls", true),
                change_info: banned("change_info", false),
                invite_users: banned("invite_users", true),
                pin_messages: banned("pin_messages", false),
                manage_topics: false,
                send_photos: false,
                send_videos: false,
                send_roundvideos: false,
                send_audios: false,
                send_voices: false,
                send_docs: false,
                send_plain: false,
                edit_rank: false,
                send_reactions: false,
                until_date,
            }),
        })
        .await
        .context("messages.editChatDefaultBannedRights failed")?;
    Ok("Default permissions updated.".to_string())
}

/// Enable or disable slow mode for a supergroup.
async fn toggle_slow_mode(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    if chat.id.kind() != PeerKind::Channel {
        return Ok("Error: slow mode is only supported for supergroups.".to_string());
    }
    if channel_is_megagroup(client, &chat).await == Some(false) {
        return Ok("Error: slow mode is only supported for supergroups.".to_string());
    }
    let seconds = i64_arg(args, "seconds", 0) as i32;
    let channel = require_channel(&chat)?;
    client
        .invoke(&tl::functions::channels::ToggleSlowMode { channel, seconds })
        .await
        .context("channels.toggleSlowMode failed")?;
    if seconds == 0 {
        Ok("Slow mode disabled.".to_string())
    } else {
        Ok(format!("Slow mode enabled (interval: {seconds}s)."))
    }
}

/// Set granular admin rights for a user in a supergroup or channel.
async fn edit_admin_rights(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let channel = require_channel(&chat)?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    let rank = str_arg(args, "rank");
    let rights = admin_rights_individual(
        bool_arg(args, "change_info", false),
        bool_arg(args, "post_messages", false),
        bool_arg(args, "edit_messages", false),
        bool_arg(args, "delete_messages", false),
        bool_arg(args, "ban_users", false),
        bool_arg(args, "invite_users", false),
        bool_arg(args, "pin_messages", false),
        bool_arg(args, "add_admins", false),
        bool_arg(args, "anonymous", false),
        bool_arg(args, "manage_call", false),
        bool_arg(args, "manage_topics", false),
        bool_arg(args, "other", false),
    );
    client
        .invoke(&tl::functions::channels::EditAdmin {
            channel,
            user_id: input_user_of(&user)?,
            admin_rights: tl::enums::ChatAdminRights::Rights(rights),
            rank: Some(rank.to_string()),
        })
        .await
        .context("channels.editAdmin failed")?;
    Ok("Admin rights updated.".to_string())
}

/// List all admins in a group or channel.
async fn get_admins(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    if chat.id.kind() == PeerKind::User {
        return Ok("Error: chat_id must be a group or channel.".to_string());
    }
    let mut iter = client
        .iter_participants(chat)
        .filter(tl::enums::ChannelParticipantsFilter::ChannelParticipantsAdmins);
    let mut lines = Vec::new();
    while let Some(p) = iter.next().await.context("Failed to fetch admins")? {
        let role = match &p.role {
            grammers_client::peer::Role::Creator(_) => " (creator)",
            grammers_client::peer::Role::Admin(_) => "",
            _ => "",
        };
        lines.push(format!("{}{role}", user_line(&p.user)));
    }
    if lines.is_empty() {
        Ok("No admins found.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

/// Get one member's role, rank and full admin-rights map in a supergroup/channel.
async fn get_member_admin_status(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let channel = require_channel(&chat)?;
    let user = resolve_user_peer(
        client,
        args.get("user_id").context("user_id is required")?,
    )
    .await?;
    let result = client
        .invoke(&tl::functions::channels::GetParticipant {
            channel,
            participant: tl::enums::InputPeer::from(&user),
        })
        .await;
    let participant = match result {
        Ok(tl::enums::channels::ChannelParticipant::Participant(p)) => Some(p.participant),
        Err(e) if e.to_string().contains("USER_NOT_PARTICIPANT") => None,
        Err(e) => return Err(anyhow::anyhow!("channels.getParticipant failed: {e}")),
    };
    let (role, rank, rights_json) = match &participant {
        None => ("not-participant", None, Value::Null),
        Some(tl::enums::ChannelParticipant::Creator(c)) => (
            "creator",
            c.rank.clone(),
            admin_rights_json(&c.admin_rights),
        ),
        Some(tl::enums::ChannelParticipant::Admin(a)) => {
            ("admin", a.rank.clone(), admin_rights_json(&a.admin_rights))
        }
        Some(tl::enums::ChannelParticipant::Banned(b)) => {
            let restricted = match &b.banned_rights {
                tl::enums::ChatBannedRights::Rights(r) => r.view_messages,
            };
            (
                if restricted { "banned" } else { "restricted" },
                None,
                Value::Null,
            )
        }
        Some(tl::enums::ChannelParticipant::Left(_)) => ("not-participant", None, Value::Null),
        _ => ("member", None, Value::Null),
    };
    Ok(serde_json::to_string_pretty(&json!({
        "role": role,
        "rank": rank,
        "admin_rights": rights_json,
    }))
    .unwrap_or_default())
}

/// Render a ChatAdminRights enum as a JSON object of bools.
fn admin_rights_json(rights: &tl::enums::ChatAdminRights) -> Value {
    match rights {
        tl::enums::ChatAdminRights::Rights(r) => json!({
            "change_info": r.change_info,
            "post_messages": r.post_messages,
            "edit_messages": r.edit_messages,
            "delete_messages": r.delete_messages,
            "ban_users": r.ban_users,
            "invite_users": r.invite_users,
            "pin_messages": r.pin_messages,
            "add_admins": r.add_admins,
            "anonymous": r.anonymous,
            "manage_call": r.manage_call,
            "other": r.other,
            "manage_topics": r.manage_topics,
            "post_stories": r.post_stories,
            "edit_stories": r.edit_stories,
            "delete_stories": r.delete_stories,
            "manage_direct_messages": r.manage_direct_messages,
        }),
    }
}

/// List all banned users in a group or channel.
async fn get_banned_users(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    if chat.id.kind() == PeerKind::User {
        return Ok("Error: chat_id must be a group or channel.".to_string());
    }
    let mut iter = client.iter_participants(chat).filter(
        tl::enums::ChannelParticipantsFilter::ChannelParticipantsKicked(
            tl::types::ChannelParticipantsKicked { q: String::new() },
        ),
    );
    let mut lines = Vec::new();
    while let Some(p) = iter.next().await.context("Failed to fetch banned users")? {
        lines.push(user_line(&p.user));
    }
    if lines.is_empty() {
        Ok("No banned users found.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

/// Export (create/refresh) the invite link for a group or channel.
async fn export_invite_link(client: &Client, chat: &PeerRef) -> anyhow::Result<String> {
    let result = client
        .invoke(&tl::functions::messages::ExportChatInvite {
            legacy_revoke_permanent: false,
            request_needed: false,
            peer: tl::enums::InputPeer::from(chat),
            expire_date: None,
            usage_limit: None,
            title: None,
            subscription_pricing: None,
        })
        .await
        .context("messages.exportChatInvite failed")?;
    match result {
        tl::enums::ExportedChatInvite::ChatInviteExported(e) => Ok(e.link),
        tl::enums::ExportedChatInvite::ChatInvitePublicJoinRequests => {
            Ok("This chat only supports public join requests (no link).".to_string())
        }
    }
}

/// Get the invite link for a group or channel.
async fn get_invite_link(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    match export_invite_link(client, &chat).await {
        Ok(link) => Ok(link),
        Err(e) => Ok(format!("Error: {e}")),
    }
}

/// Export a chat invite link (alias-style duplicate of the invite export call).
async fn export_chat_invite(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    match export_invite_link(client, &chat).await {
        Ok(link) => Ok(link),
        Err(e) => Ok(format!("Error: {e}")),
    }
}

/// Shared core: check an invite hash, then import it.
async fn join_by_hash(client: &Client, hash: &str) -> anyhow::Result<String> {
    // Check first: returns the chat when we are already a member.
    if let Ok(info) = client
        .invoke(&tl::functions::messages::CheckChatInvite {
            hash: hash.to_string(),
        })
        .await
    {
        if let tl::enums::ChatInvite::Already(a) = info {
            let title = match &a.chat {
                tl::enums::Chat::Chat(c) => c.title.clone(),
                tl::enums::Chat::Channel(c) => c.title.clone(),
                _ => "Unknown Chat".to_string(),
            };
            return Ok(format!("You are already a member of this chat: {title}"));
        }
    }
    let result = client
        .invoke(&tl::functions::messages::ImportChatInvite {
            hash: hash.to_string(),
        })
        .await;
    match result {
        Ok(tl::enums::messages::ChatInviteJoinResult::Ok(_)) => {
            Ok("Successfully joined chat via invite.".to_string())
        }
        Ok(tl::enums::messages::ChatInviteJoinResult::WebView(_)) => Ok(
            "Join request sent; the chat requires admin approval or a webview flow.".to_string(),
        ),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("expired") {
                Ok("The invite hash has expired and is no longer valid.".to_string())
            } else if msg.contains("invalid") {
                Ok("The invite hash is invalid or malformed.".to_string())
            } else if msg.contains("already") && msg.contains("participant") {
                Ok("You are already a member of this chat.".to_string())
            } else if msg.contains("admin") {
                Ok("Cannot join this chat - requires admin approval.".to_string())
            } else if msg.contains("too much") || msg.contains("too many") {
                Ok("Cannot join this chat - it has reached maximum number of participants."
                    .to_string())
            } else {
                Err(anyhow::anyhow!("messages.importChatInvite failed: {e}"))
            }
        }
    }
}

/// Join a chat by invite link.
async fn join_chat_by_link(client: &Client, args: &Value) -> anyhow::Result<String> {
    let link = str_arg(args, "link");
    if link.is_empty() {
        bail!("link is required");
    }
    join_by_hash(client, &invite_hash(link)).await
}

/// Import a chat invite by hash.
async fn import_chat_invite(client: &Client, args: &Value) -> anyhow::Result<String> {
    let hash = str_arg(args, "hash");
    if hash.is_empty() {
        bail!("hash is required");
    }
    join_by_hash(client, &invite_hash(hash)).await
}

/// Get recent admin actions (admin log) in a group or channel.
async fn get_recent_actions(client: &Client, args: &Value) -> anyhow::Result<String> {
    let chat = resolve_peer(client, args.get("chat_id").context("chat_id is required")?).await?;
    let channel = require_channel(&chat)?;
    let name = peer_display_name(client, &chat).await;
    let result = client
        .invoke(&tl::functions::channels::GetAdminLog {
            channel,
            q: String::new(),
            events_filter: None,
            admins: None,
            max_id: 0,
            min_id: 0,
            limit: 20,
        })
        .await
        .context("channels.getAdminLog failed")?;
    match result {
        tl::enums::channels::AdminLogResults::Results(r) => {
            if r.events.is_empty() {
                return Ok("No recent admin actions found.".to_string());
            }
            let mut lines = vec![format!("Recent admin actions in {name}:")];
            for event in &r.events {
                let action = admin_log_action_name(&event.action);
                lines.push(format!(
                    "- event {} | user {} | {action} | at {}",
                    event.id, event.user_id, event.date
                ));
            }
            Ok(lines.join("\n"))
        }
    }
}

// ---------------------------------------------------------------------------
// definitions & dispatch
// ---------------------------------------------------------------------------

fn chat_id_prop(desc: &str) -> Value {
    json!({
        "type": ["integer", "string"],
        "description": desc,
    })
}

fn user_id_prop(desc: &str) -> Value {
    json!({
        "type": ["integer", "string"],
        "description": desc,
    })
}

/// All 25 group/channel management tools.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "create_group".into(),
            description: "Create a new group and add users. user_ids accepts user IDs or @usernames."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Title for the new group"},
                    "user_ids": {"type": "array", "items": {"type": ["integer", "string"]}, "description": "User IDs or @usernames to add"}
                },
                "required": ["title", "user_ids"]
            }),
        },
        ToolDefinition {
            name: "invite_to_group".into(),
            description: "Invite users to a group or channel. Accepts user IDs or @usernames."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group_id": chat_id_prop("ID or @username of the group/channel"),
                    "user_ids": {"type": "array", "items": {"type": ["integer", "string"]}, "description": "User IDs or @usernames to invite"}
                },
                "required": ["group_id", "user_ids"]
            }),
        },
        ToolDefinition {
            name: "leave_chat".into(),
            description: "Leave a group or channel by chat ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Chat ID or @username to leave")
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "get_participants".into(),
            description: "List participants in a group or channel with pagination (page is 1-indexed, page_size default 200, max 1000)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Group or channel ID or @username"),
                    "page": {"type": "integer", "description": "Page number (1-indexed, default 1)"},
                    "page_size": {"type": "integer", "description": "Participants per page (default 200, max 1000)"}
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "create_channel".into(),
            description: "Create a new channel or supergroup (megagroup=true for a supergroup)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Channel title"},
                    "about": {"type": "string", "description": "Channel description (optional)"},
                    "megagroup": {"type": "boolean", "description": "Create a supergroup instead of a broadcast channel (default false)"}
                },
                "required": ["title"]
            }),
        },
        ToolDefinition {
            name: "edit_chat_title".into(),
            description: "Edit the title of a chat, group, or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Chat ID or @username"),
                    "title": {"type": "string", "description": "New title"}
                },
                "required": ["chat_id", "title"]
            }),
        },
        ToolDefinition {
            name: "edit_chat_photo".into(),
            description: "Edit the photo of a chat, group, or channel from a local image file."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Chat ID or @username"),
                    "file_path": {"type": "string", "description": "Local path to an image file"}
                },
                "required": ["chat_id", "file_path"]
            }),
        },
        ToolDefinition {
            name: "edit_chat_about".into(),
            description: "Edit the description (About) of a chat, group, or channel (max 255 characters)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Chat ID or @username"),
                    "about": {"type": "string", "description": "New description text"}
                },
                "required": ["chat_id", "about"]
            }),
        },
        ToolDefinition {
            name: "delete_chat_photo".into(),
            description: "Delete the photo of a chat, group, or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Chat ID or @username")
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "promote_admin".into(),
            description: "Promote a user to admin in a group/channel with broad rights by default. Pass a rights object to customize individual flags."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group_id": chat_id_prop("ID or @username of the group/channel"),
                    "user_id": user_id_prop("User ID or @username to promote"),
                    "rank": {"type": "string", "description": "Custom admin title (default 'Admin')"},
                    "rights": {"type": "object", "description": "Optional rights overrides: change_info, post_messages, edit_messages, delete_messages, ban_users, invite_users, pin_messages, add_admins, anonymous, manage_call, manage_topics, other"}
                },
                "required": ["group_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "demote_admin".into(),
            description: "Demote a user from admin in a group/channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group_id": chat_id_prop("ID or @username of the group/channel"),
                    "user_id": user_id_prop("User ID or @username to demote")
                },
                "required": ["group_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "ban_user".into(),
            description: "Permanently ban a user from a group or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the group/channel"),
                    "user_id": user_id_prop("User ID or @username to ban")
                },
                "required": ["chat_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "unban_user".into(),
            description: "Unban a user from a group or channel (clear all restrictions).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the group/channel"),
                    "user_id": user_id_prop("User ID or @username to unban")
                },
                "required": ["chat_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "remove_user".into(),
            description: "Remove a user from a group or channel WITHOUT banning them (no ban left in place)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the group/channel"),
                    "user_id": user_id_prop("User ID or @username to remove")
                },
                "required": ["chat_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "set_default_chat_permissions".into(),
            description: "Set default member permissions for a group/supergroup/channel. Pass true to allow, false to restrict each capability."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the chat"),
                    "send_messages": {"type": "boolean", "default": true},
                    "send_media": {"type": "boolean", "default": true},
                    "send_stickers": {"type": "boolean", "default": true},
                    "send_gifs": {"type": "boolean", "default": true},
                    "send_games": {"type": "boolean", "default": true},
                    "send_inline": {"type": "boolean", "default": true},
                    "embed_links": {"type": "boolean", "default": true},
                    "send_polls": {"type": "boolean", "default": true},
                    "change_info": {"type": "boolean", "default": false},
                    "invite_users": {"type": "boolean", "default": true},
                    "pin_messages": {"type": "boolean", "default": false},
                    "until_date": {"type": "integer", "description": "Restriction expiry as Unix timestamp, 0 = permanent (default)"}
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "toggle_slow_mode".into(),
            description: "Enable or disable slow mode for a supergroup. seconds in {0, 10, 30, 60, 300, 900, 3600}; 0 disables."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the supergroup"),
                    "seconds": {"type": "integer", "description": "Interval between messages per user. 0 = disabled (default)"}
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "edit_admin_rights".into(),
            description: "Set granular admin rights for a user in a supergroup or channel. Pass true to grant, false to revoke. All false revokes admin status."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the supergroup/channel"),
                    "user_id": user_id_prop("User ID or @username"),
                    "rank": {"type": "string", "description": "Custom admin title (max 16 chars). Empty = no custom title"},
                    "change_info": {"type": "boolean", "default": false},
                    "post_messages": {"type": "boolean", "default": false},
                    "edit_messages": {"type": "boolean", "default": false},
                    "delete_messages": {"type": "boolean", "default": false},
                    "ban_users": {"type": "boolean", "default": false},
                    "invite_users": {"type": "boolean", "default": false},
                    "pin_messages": {"type": "boolean", "default": false},
                    "add_admins": {"type": "boolean", "default": false},
                    "anonymous": {"type": "boolean", "default": false},
                    "manage_call": {"type": "boolean", "default": false},
                    "manage_topics": {"type": "boolean", "default": false},
                    "other": {"type": "boolean", "default": false}
                },
                "required": ["chat_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "get_admins".into(),
            description: "Get all admins in a group or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Group or channel ID or @username")
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "get_member_admin_status".into(),
            description: "Get one member's role (creator/admin/member/restricted/banned/not-participant), rank and full admin-rights map in a supergroup or channel."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("ID or @username of the supergroup/channel"),
                    "user_id": user_id_prop("User ID or @username of the member")
                },
                "required": ["chat_id", "user_id"]
            }),
        },
        ToolDefinition {
            name: "get_banned_users".into(),
            description: "Get all banned users in a group or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Group or channel ID or @username")
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "get_invite_link".into(),
            description: "Get (export) the invite link for a group or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Group or channel ID or @username")
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "join_chat_by_link".into(),
            description: "Join a chat by invite link (t.me/... or bare hash).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "link": {"type": "string", "description": "Invite link or hash"}
                },
                "required": ["link"]
            }),
        },
        ToolDefinition {
            name: "export_chat_invite".into(),
            description: "Export a chat invite link for a group or channel.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Group or channel ID or @username")
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "import_chat_invite".into(),
            description: "Import (join via) a chat invite by hash.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "hash": {"type": "string", "description": "Invite hash"}
                },
                "required": ["hash"]
            }),
        },
        ToolDefinition {
            name: "get_recent_actions".into(),
            description: "Get recent admin actions (admin log) in a group or channel (latest 20)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": chat_id_prop("Group or channel ID or @username")
                },
                "required": ["chat_id"]
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
        "create_group" => create_group(client, args).await?,
        "invite_to_group" => invite_to_group(client, args).await?,
        "leave_chat" => leave_chat(client, args).await?,
        "get_participants" => get_participants(client, args).await?,
        "create_channel" => create_channel(client, args).await?,
        "edit_chat_title" => edit_chat_title(client, args).await?,
        "edit_chat_photo" => edit_chat_photo(client, args).await?,
        "edit_chat_about" => edit_chat_about(client, args).await?,
        "delete_chat_photo" => delete_chat_photo(client, args).await?,
        "promote_admin" => promote_admin(client, args).await?,
        "demote_admin" => demote_admin(client, args).await?,
        "ban_user" => ban_user(client, args).await?,
        "unban_user" => unban_user(client, args).await?,
        "remove_user" => remove_user(client, args).await?,
        "set_default_chat_permissions" => set_default_chat_permissions(client, args).await?,
        "toggle_slow_mode" => toggle_slow_mode(client, args).await?,
        "edit_admin_rights" => edit_admin_rights(client, args).await?,
        "get_admins" => get_admins(client, args).await?,
        "get_member_admin_status" => get_member_admin_status(client, args).await?,
        "get_banned_users" => get_banned_users(client, args).await?,
        "get_invite_link" => get_invite_link(client, args).await?,
        "join_chat_by_link" => join_chat_by_link(client, args).await?,
        "export_chat_invite" => export_chat_invite(client, args).await?,
        "import_chat_invite" => import_chat_invite(client, args).await?,
        "get_recent_actions" => get_recent_actions(client, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(CallToolResult::ok(text)))
}
