# Telegram MCP

<p align="center">
  <img src="assets/icon.png" width="128" alt="Telegram MCP Icon">
</p>

[中文版](README.zh-CN.md)

Lightweight Telegram MCP server in Rust. Sign in with your Telegram account to read all chats, groups and channels you're in (including private groups).

## Architecture

```
┌─────────────────┐     stdio/JSON-RPC     ┌──────────────────┐     MTProto     ┌─────────────────┐
│   MCP Client    │ ◄────────────────────► │   telegram-mcp   │ ◄─────────────► │ Telegram Servers│
│ Claude/Cursor/… │                        │   (Rust binary)  │                 │                 │
└─────────────────┘                        └──────────────────┘                 └─────────────────┘
```

## Features

<details>
<summary><b>All 124 Tools by Module</b></summary>

<details>
<summary>Messages (25)</summary>

| Tool | Description |
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
<summary>Groups (30)</summary>

| Tool | Description |
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
<summary>Chats (13)</summary>

| Tool | Description |
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
<summary>Media (12)</summary>

| Tool | Description |
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
<summary>Contacts (16)</summary>

| Tool | Description |
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
<summary>Profile (8)</summary>

| Tool | Description |
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
<summary>Folders (10)</summary>

| Tool | Description |
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
<summary>Accounts (1)</summary>

| Tool | Description |
|------|-------------|
| `list_accounts` | List the configured Telegram account with profile info (name |

</details>

<details>
<summary>Events (3)</summary>

| Tool | Description |
|------|-------------|
| `disable_incoming_feed` | Disable the incoming event feed (stops writing to the feed f |
| `enable_incoming_feed` | Enable callback mode: a background task appends every settle |
| `incoming_feed_status` | Report whether the incoming event feed is enabled, its file  |

</details>

<details>
<summary>Legacy (4)</summary>

| Tool | Description |
|------|-------------|
| `get_messages_context` | Get context messages around a specific message |
| `get_profile` | Get user, group or channel profile info |
| `join_channel` | Join a public channel by @username |
| `search_in_channel` | Search keywords within a specific channel or group |

</details>

<details>
<summary>Other (2)</summary>

| Tool | Description |
|------|-------------|
| `get_last_interaction` | Get the most recent messages with a contact |
| `get_send_as` | List Telegram's allowed send-as peers for a chat (peer IDs,  |

</details>

</details>

## Quick Start

> 🤖 **Let your AI do it**: Copy-paste to your AI assistant:
> ```
> Read https://github.com/zxsos/Telegram-mcp/blob/master/AGENT.md and install Telegram MCP for me.
> ```

```bash
# 1. Get Telegram API credentials from https://my.telegram.org/apps
# 2. Build
cargo build --release

# 3. Login (one time, needs phone verification code)
./target/release/telegram-mcp --auth
# Code will be sent to your phone/Telegram app.
# - Interactive: type the code when prompted
# - Non-interactive: set TG_CODE env var, or write code to /tmp/telegram_mcp_code
# - 2FA: set TG_2FA_PASSWORD env var if enabled

# Behind SOCKS5 proxy (optional)
./target/release/telegram-mcp --proxy socks5://user:pass@host:port --auth
# Or set TG_PROXY env var:
# TG_PROXY=socks5://user:pass@host:port ./target/release/telegram-mcp --auth

# 4. Add to MCP client

**Claude Code** (CLI):
```bash
claude mcp add --transport stdio telegram -- /absolute/path/to/telegram-mcp
```

**Claude Desktop / Cursor / Windsurf / Cline** — add to config JSON:
```json
{
  "mcpServers": {
    "telegram": {
      "command": "/absolute/path/to/telegram-mcp",
      "env": {
        "TG_API_ID": "your_api_id",
        "TG_API_HASH": "your_api_hash"
      }
    }
  }
}
```
Config file locations:
- Claude Desktop (macOS): `~/Library/Application Support/Claude/claude_desktop_config.json`
- Claude Desktop (Windows): `%APPDATA%\Claude\claude_desktop_config.json`
- Cursor: `~/.cursor/mcp.json` (global) or `.cursor/mcp.json` (project)
- Windsurf: `~/.codeium/windsurf/mcp_config.json`

**VS Code** (`.vscode/mcp.json`) — note the different format:
```json
{
  "servers": {
    "telegram": {
      "type": "stdio",
      "command": "/absolute/path/to/telegram-mcp",
      "env": {
        "TG_API_ID": "your_api_id",
        "TG_API_HASH": "your_api_hash"
      }
    }
  }
}
```

**Zed** (`~/.config/zed/settings.json`):
```json
{
  "context_servers": {
    "telegram": {
      "command": {
        "path": "/absolute/path/to/telegram-mcp",
        "args": []
      },
      "env": {
        "TG_API_ID": "your_api_id",
        "TG_API_HASH": "your_api_hash"
      }
    }
  }
}
```

**OpenAI Codex** (`~/.codex/config.toml`):
```toml
[mcp_servers.telegram]
command = "/absolute/path/to/telegram-mcp"

[mcp_servers.telegram.env]
TG_API_ID = "your_api_id"
TG_API_HASH = "your_api_hash"
```
Or via CLI: `codex mcp add telegram --command /absolute/path/to/telegram-mcp`

**Tencent WorkBuddy** (`~/.workbuddy/.mcp.json`):
```json
{
  "mcpServers": {
    "telegram": {
      "command": "/absolute/path/to/telegram-mcp",
      "env": {
        "TG_API_ID": "your_api_id",
        "TG_API_HASH": "your_api_hash"
      }
    }
  }
}
```

**Tencent CodeBuddy CLI** (`~/.codebuddy/.mcp.json`):
```json
{
  "mcpServers": {
    "telegram": {
      "type": "stdio",
      "command": "/absolute/path/to/telegram-mcp",
      "env": {
        "TG_API_ID": "your_api_id",
        "TG_API_HASH": "your_api_hash"
      }
    }
  }
}
```

> Tip: Use the absolute path to the binary (e.g. `/home/user/telegram-mcp/target/release/telegram-mcp`). GUI clients may not inherit your shell's PATH.

### Usage Examples

Once connected, just talk to your AI assistant naturally:

- "List my recent chats"
- "Show me unread messages from the last hour"
- "Send 'hello' to @username"
- "Search for messages about 'meeting' in my groups"
- "Download the latest photo from my Saved Messages"

## License

Apache-2.0
