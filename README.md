# Telegram MCP

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

| Tool | Description |
|------|-------------|
| `list_chats` | List all dialogs (groups/channels/DMs) with IDs |
| `get_messages` | Read recent messages from a chat by ID |
| `search_in_channel` | Search keywords within a channel |
| `get_messages_context` | Read context around a message |
| `get_profile` | Get user/group/channel info |
| `join_channel` | Join a public channel |
| `send_message` | Send a message |
| `download_media` | Download photos/videos/files |
| `search_messages` | Global message search |
| ... | 124 tools total (messages, groups, media, contacts, admin) |

## Quick Start

```bash
# 1. Get Telegram API credentials from https://my.telegram.org/apps
# 2. Build
cargo build --release

# 3. Login (one time, needs phone verification code)
./target/release/telegram-mcp --auth

# Non-interactive login (for scripts/CI)
export TG_API_ID='12345'
export TG_API_HASH='abcdef...'
export TG_PHONE='+1234567890'
export TG_CODE='123456'          # Code from Telegram
export TG_2FA_PASSWORD='...'     # If 2FA enabled
./target/release/telegram-mcp --auth

# Behind HTTP proxy (auto-uses $http_proxy)
./target/release/telegram-mcp-proxy --auth

# 4. Add to MCP client
```

Claude Code:
```bash
claude mcp add telegram -- ./target/release/telegram-mcp
```

## Monitor Group Messages

```
1. Call list_chats to find the target group's numeric ID
2. Poll get_messages(chat_id="xxx", limit=20) for new messages
3. Filter by keywords
```

## License

MIT
