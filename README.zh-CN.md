# Telegram MCP

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

| 工具 | 说明 |
|------|------|
| `list_chats` | 列出所有对话（群/频道/私聊），返回 ID |
| `get_messages` | 按 ID 读指定聊天的最新消息 |
| `search_in_channel` | 在指定频道内搜关键词 |
| `get_messages_context` | 读某条消息的上下文 |
| `get_profile` | 查用户/群/频道资料 |
| `join_channel` | 加入公开频道 |
| `send_message` | 发消息 |
| `download_media` | 下载图片/视频/文件 |
| `search_messages` | 全局搜消息 |
| ... | 共 124 个工具（消息、群组、媒体、联系人、管理） |

## 快速开始

```bash
# 1. 去 https://my.telegram.org/apps 拿 API 凭证
# 2. 编译
cargo build --release

# 3. 登录（只需一次，需手机验证码）
./target/release/telegram-mcp --auth

# 非交互登录（脚本/CI 用）
export TG_API_ID='12345'
export TG_API_HASH='abcdef...'
export TG_PHONE='+1234567890'
export TG_CODE='123456'          # Telegram 验证码
export TG_2FA_PASSWORD='...'     # 如开了 2FA
./target/release/telegram-mcp --auth

# 在 HTTP 代理后（自动用 $http_proxy）
./scripts/telegram-mcp-proxy --auth

# 4. 接入 MCP 客户端
```

Claude Code：
```bash
claude mcp add telegram -- ./target/release/telegram-mcp
```

## 监控群新消息

```
1. 调 list_chats 找到目标群的数字 ID
2. 定时调 get_messages(chat_id="xxx", limit=20) 拉新消息
3. 按关键词过滤
```

## License

MIT
