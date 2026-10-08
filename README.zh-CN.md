# Telegram MCP

<img src="assets/icon.png" width="128" alt="Telegram MCP Icon">

[English](README.md)

轻量 Telegram MCP 服务器（Rust），用你的 Telegram 账号登录，可读你加入的所有群/频道（含私密群）。

## 架构

```
┌─────────────────┐     stdio/JSON-RPC     ┌──────────────────┐     MTProto     ┌─────────────────┐
│   MCP 客户端    │ ◄────────────────────► │   telegram-mcp   │ ◄─────────────► │  Telegram 服务器│
│ Claude/Cursor/… │                        │   (Rust 二进制)  │                 │                 │
└─────────────────┘                        └──────────────────┘                 └─────────────────┘
```

## 功能

<details>
<summary><b>全部 124 个工具（按模块展开）</b></summary>

<details>
<summary>消息 (25)</summary>

| 工具 | 说明 |
|------|-------------|
| `delete_message` | Delete a message by ID |
| `delete_messages_bulk` | Delete multiple messages in a single call |
| `delete_scheduled_message` | Delete one or more scheduled (pending) messages from a chat |
| `edit_message` | Edit a message you sent |
| `forward_message` | Forward message(s) from one chat to another |
| `forward_messages` | Forward a batch of messages from one chat to another in a si |
| `get_message_context` | Retrieve messages around a specific message ID, with the tar |
| `get_message_link` | Export a t |
| `get_message_read_by` | List user IDs who have read a specific message (small groups |
| `get_messages` | Get paginated messages from a chat, newest first |
| `get_scheduled_messages` | List all scheduled (pending) messages in a chat |
| `list_inline_buttons` | Inspect inline buttons on a message to discover their indice |
| `list_messages` | Retrieve messages with optional text search and date filters |
| `pin_message` | Pin a message in a chat |
| `press_inline_button` | Press an inline button (callback) on a chat message |
| `send_album` | Send 2-10 photos/videos as one Telegram media group (album) |
| `send_file` | Send a file to a chat |
| `send_gif` | Send a GIF to a chat by Telegram GIF document ID |
| `send_message` | Send a message to a chat |
| `send_scheduled_message` | Schedule a message to be sent at a future time |
| `send_sticker` | Send a sticker to a chat |
| `send_voice` | Send a voice message to a chat |
| `transcribe_voice` | Transcribe a voice message or video note to text |
| `wait_for_new_message` | Block until a new incoming private message from a non-bot us |
| `wait_for_settled_message` | Event-driven DEBOUNCED wait |

</details>

<details>
<summary>群组管理 (30)</summary>

| 工具 | 说明 |
|------|-------------|
| `ban_user` | Permanently ban a user from a group or channel |
| `create_channel` | Create a new channel or supergroup (megagroup=true for a sup |
| `create_forum_topic` | Create a forum topic in a forum-enabled supergroup |
| `create_group` | Create a new group and add users |
| `delete_chat_photo` | Delete the photo of a chat, group, or channel |
| `delete_forum_topic` | Delete a forum topic together with all its messages |
| `demote_admin` | Demote a user from admin in a group/channel |
| `edit_admin_rights` | Set granular admin rights for a user in a supergroup or chan |
| `edit_chat_about` | Edit the description (About) of a chat, group, or channel (m |
| `edit_chat_photo` | Edit the photo of a chat, group, or channel from a local ima |
| `edit_chat_title` | Edit the title of a chat, group, or channel |
| `edit_forum_topic` | Edit a forum topic |
| `enable_forum_topics` | Enable forum topics for a supergroup |
| `export_chat_invite` | Export a chat invite link for a group or channel |
| `get_admins` | Get all admins in a group or channel |
| `get_banned_users` | Get all banned users in a group or channel |
| `get_invite_link` | Get (export) the invite link for a group or channel |
| `get_member_admin_status` | Get one member's role (creator/admin/member/restricted/banne |
| `get_participants` | List participants in a group or channel with pagination (pag |
| `get_recent_actions` | Get recent admin actions (admin log) in a group or channel ( |
| `import_chat_invite` | Import (join via) a chat invite by hash |
| `invite_to_group` | Invite users to a group or channel |
| `join_chat_by_link` | Join a chat by invite link (t |
| `leave_chat` | Leave a group or channel by chat ID |
| `list_topics` | List forum topics of a forum-enabled supergroup |
| `promote_admin` | Promote a user to admin in a group/channel with broad rights |
| `remove_user` | Remove a user from a group or channel WITHOUT banning them ( |
| `set_default_chat_permissions` | Set default member permissions for a group/supergroup/channe |
| `toggle_slow_mode` | Enable or disable slow mode for a supergroup |
| `unban_user` | Unban a user from a group or channel (clear all restrictions |

</details>

<details>
<summary>聊天 (13)</summary>

| 工具 | 说明 |
|------|-------------|
| `archive_chat` | Archive a chat |
| `delete_chat_history` | Clear the message history of a chat |
| `get_chat` | Get detailed information about a specific chat by ID or user |
| `get_chats` | Get a paginated list of chats (dialogs) with IDs and titles |
| `get_common_chats` | List chats shared with a specific user |
| `get_full_chat` | Get full info of a channel or group, including description/a |
| `list_chats` | List chats with metadata (type, unread, muted, archived) |
| `mute_chat` | Mute notifications for a chat |
| `resolve_username` | Resolve a username to a user or chat ID |
| `search_public_chats` | Search for public chats, channels, or bots by username or ti |
| `subscribe_public_channel` | Subscribe (join) a public channel or supergroup by username  |
| `unarchive_chat` | Unarchive a chat |
| `unmute_chat` | Unmute notifications for a chat |

</details>

<details>
<summary>媒体 (12)</summary>

| 工具 | 说明 |
|------|-------------|
| `delete_profile_photo` | Delete your current profile photo |
| `download_media` | Download media from a message in a chat to a local file |
| `get_gif_search` | Search for GIFs by query via the @gif inline bot |
| `get_media_info` | Get info about media in a message (type, id, name, mime type |
| `get_photo_sheet` | Download thumbnails of many photos of a peer (max 12) and re |
| `get_sticker_sets` | Get all installed sticker sets (titles) |
| `get_user_photos` | Get profile photo IDs of a user (newest first) |
| `inspect_document` | Inspect a document/image from a Telegram message, downloaded |
| `list_photos` | Index a peer's photos as text without transferring images |
| `open_photo` | Download one photo of a peer (avatar via photo_id, or chat p |
| `set_profile_photo` | Set a new profile photo from a local image file path |
| `upload_file` | Upload a local file to Telegram and return upload metadata ( |

</details>

<details>
<summary>联系人 (16)</summary>

| 工具 | 说明 |
|------|-------------|
| `add_contact` | Add a new contact to your Telegram account |
| `block_user` | Block a user by user ID or username |
| `delete_contact` | Delete a contact by user ID or username |
| `delete_contact_alias` | Forget one remembered alias (local-only) |
| `export_contacts` | Export all contacts as a JSON string |
| `get_blocked_users` | Get a list of blocked users |
| `get_contact_chats` | List all chats involving a specific contact (direct chat plu |
| `get_contact_ids` | Get all contact IDs in your Telegram account |
| `get_direct_chat_by_contact` | Find a direct chat with a specific contact by name, username |
| `import_contacts` | Import a list of contacts |
| `list_contact_aliases` | List remembered contact aliases (local-only) |
| `list_contacts` | List all contacts in your Telegram account |
| `search_contacts` | Search for contacts by name, username, or phone number using |
| `send_contact` | Send a contact card to a chat |
| `set_contact_alias` | Remember what the user calls someone (local-only alias) |
| `unblock_user` | Unblock a user by user ID or username |

</details>

<details>
<summary>资料 (8)</summary>

| 工具 | 说明 |
|------|-------------|
| `get_bot_info` | Get information about a bot by username |
| `get_full_user` | Get full profile info of a Telegram user: bio, channel link, |
| `get_me` | Get your own Telegram user information |
| `get_privacy_settings` | Get your privacy settings for last seen status |
| `get_user_status` | Get the online/last-seen status of a user |
| `set_bot_commands` | Set bot commands for a bot you own |
| `set_privacy_settings` | Set a privacy rule |
| `update_profile` | Update your profile information (name, bio) |

</details>

<details>
<summary>文件夹 (10)</summary>

| 工具 | 说明 |
|------|-------------|
| `add_chat_to_folder` | Add a chat (bare numeric id or @username) to an existing fol |
| `create_folder` | Create a new dialog folder |
| `delete_folder` | Delete a folder |
| `get_folder` | Get detailed information about a specific folder including a |
| `get_folder_limits` | Read effective Premium-aware folder, explicit-chat and pin l |
| `get_folder_snapshot` | Read complete folder definitions and order without chat cont |
| `list_folders` | Get all dialog folders (filters) with their IDs, names, and  |
| `remove_chat_from_folder` | Remove a chat (bare numeric id or @username) from a folder |
| `reorder_folders` | Change the order of folders |
| `update_folder` | Patch an existing PRIVATE folder by ID, preserving every omi |

</details>

<details>
<summary>账号 (1)</summary>

| 工具 | 说明 |
|------|-------------|
| `list_accounts` | List the configured Telegram account with profile info (name |

</details>

<details>
<summary>事件 (3)</summary>

| 工具 | 说明 |
|------|-------------|
| `disable_incoming_feed` | Disable the incoming event feed (stops writing to the feed f |
| `enable_incoming_feed` | Enable callback mode: a background task appends every settle |
| `incoming_feed_status` | Report whether the incoming event feed is enabled, its file  |

</details>

<details>
<summary>兼容接口 (4)</summary>

| 工具 | 说明 |
|------|-------------|
| `get_messages_context` | Get context messages around a specific message |
| `get_profile` | Get user, group or channel profile info |
| `join_channel` | Join a public channel by @username |
| `search_in_channel` | Search keywords within a specific channel or group |

</details>

<details>
<summary>Other 其他 (2)</summary>

| 工具 | 说明 |
|------|-------------|
| `get_last_interaction` | Get the most recent messages with a contact |
| `get_send_as` | List Telegram's allowed send-as peers for a chat (peer IDs,  |

</details>

</details>

## 快速开始

```bash
# 1. 去 https://my.telegram.org/apps 拿 API 凭证
# 2. 编译
cargo build --release

# 3. 登录（只需一次，需手机验证码）
./target/release/telegram-mcp --auth
# 验证码会发到你手机/Telegram App。
# - 交互式：按提示输入验证码
# - 非交互：设置 TG_CODE 环境变量，或把验证码写到 /tmp/telegram_mcp_code
# - 2FA：如开了两步验证，设置 TG_2FA_PASSWORD 环境变量

# 在 HTTP 代理后（自动用 $http_proxy）
./scripts/telegram-mcp-proxy --auth

# 4. 接入 MCP 客户端
```

Claude Code：
```bash
claude mcp add telegram -- ./target/release/telegram-mcp
```

## License

Apache-2.0
