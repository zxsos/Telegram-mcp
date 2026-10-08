# Telegram MCP

<p align="center">
  <img src="assets/icon.png" width="128" alt="Telegram MCP Icon">
</p>

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
<summary>消息 (23)</summary>

| 工具 | 说明 |
|------|------|
| `delete_message` | 按 ID 删除一条消息 |
| `delete_messages_bulk` | 一次调用批量删除多条消息 |
| `delete_scheduled_message` | 删除聊天中的一条或多条定时（待发送）消息 |
| `edit_message` | 编辑你发过的消息 |
| `forward_message` | 把消息从一个聊天转发到另一个，单个 message_id 会自动展开 |
| `forward_messages` | 一次原子调用批量转发多条消息 |
| `get_message_context` | 获取指定消息 ID 前后的消息，目标消息会被标记 |
| `get_message_link` | 导出消息的 t.me/... 链接，仅频道和超级群可用 |
| `get_message_read_by` | 列出已读指定消息的用户 ID（小群/超级群、近期消息） |
| `get_messages` | 分页获取聊天消息，最新在前 |
| `get_scheduled_messages` | 列出聊天中的所有定时（待发送）消息 |
| `list_inline_buttons` | 查看消息上的内联按钮，获取索引、文本和链接 |
| `list_messages` | 按条件检索消息，支持文本搜索和日期过滤（YYYY-MM-DD） |
| `pin_message` | 置顶聊天中的一条消息 |
| `press_inline_button` | 点击聊天消息上的内联按钮（回调） |
| `send_album` | 一次发送 2-10 张照片/视频为一个媒体组（相册） |
| `send_file` | 向聊天发送文件，传 2-10 个路径可一次发为媒体组 |
| `send_gif` | 按 Telegram GIF 文档 ID 发送 GIF，此构建暂不支持 |
| `send_message` | 向聊天发送消息，parse_mode 支持 md/html 但按纯文本发送 |
| `send_scheduled_message` | 定时在未来某个时间发送消息 |
| `send_sticker` | 向聊天发送贴纸，文件须为 .webp 贴纸文件 |
| `send_voice` | 向聊天发送语音消息，文件须为 OGG/OPUS 语音 |
| `transcribe_voice` | 把语音消息或视频留言转写为文字，需外部服务，此构建不支持 |

</details>

<details>
<summary>群组管理 (30)</summary>

| 工具 | 说明 |
|------|------|
| `ban_user` | 从群组或频道永久封禁用户 |
| `create_channel` | 创建新频道或超级群（megagroup=true 为超级群） |
| `create_forum_topic` | 在启用话题的超级群里创建话题 |
| `create_group` | 创建新群组并添加用户，user_ids 可用用户 ID 或 @username |
| `delete_chat_photo` | 删除聊天、群组或频道的头像 |
| `delete_forum_topic` | 删除话题及其中所有消息，不可撤销 |
| `demote_admin` | 撤销用户在群组/频道的管理员身份 |
| `edit_admin_rights` | 设置用户在超级群/频道的细粒度管理权限，传 true 授予 |
| `edit_chat_about` | 编辑聊天、群组或频道的简介（最多 255 字符） |
| `edit_chat_photo` | 用本地图片文件更换聊天、群组或频道的头像 |
| `edit_chat_title` | 编辑聊天、群组或频道的标题 |
| `edit_forum_topic` | 编辑话题，只传要改的字段 |
| `enable_forum_topics` | 为超级群启用话题功能，需要管理聊天信息的权限 |
| `export_chat_invite` | 导出群组或频道的邀请链接 |
| `get_admins` | 获取群组或频道的所有管理员 |
| `get_banned_users` | 获取群组或频道的所有被封禁用户 |
| `get_invite_link` | 获取（导出）群组或频道的邀请链接 |
| `get_member_admin_status` | 获取单个成员的角色（创建者/管理员/成员/受限/被封禁等） |
| `get_participants` | 分页列出群组或频道的成员（page 从 1 开始） |
| `get_recent_actions` | 获取群组或频道的近期管理操作日志（最近 20 条） |
| `import_chat_invite` | 通过 hash 导入（加入）聊天邀请 |
| `invite_to_group` | 邀请用户加入群组或频道，可用用户 ID 或 @username |
| `join_chat_by_link` | 通过邀请链接（t.me/... 或裸 hash）加入聊天 |
| `leave_chat` | 按聊天 ID 退出群组或频道 |
| `list_topics` | 列出启用话题的超级群的话题，传 topic_id 查单个 |
| `promote_admin` | 把用户提为群组/频道管理员，默认给较广权限 |
| `remove_user` | 把用户移出群组或频道但不封禁（不留封禁记录） |
| `set_default_chat_permissions` | 设置群组/超级群/频道的默认成员权限，传 true 允许 |
| `toggle_slow_mode` | 为超级群开启或关闭慢速模式，seconds 取值 0/10/30/60/300/900 |
| `unban_user` | 解封群组或频道中的用户（清除所有限制） |

</details>

<details>
<summary>聊天 (13)</summary>

| 工具 | 说明 |
|------|------|
| `archive_chat` | 归档一个聊天 |
| `delete_chat_history` | 清空一个聊天的消息记录 |
| `get_chat` | 按 ID 或用户名获取指定聊天的详细信息 |
| `get_chats` | 分页获取聊天（会话）列表，含 ID 和标题 |
| `get_common_chats` | 列出与指定用户的共同聊天 |
| `get_full_chat` | 获取频道或群组的完整信息，含简介文本 |
| `list_chats` | 列出聊天及元数据（类型、未读、静音、归档），用于找私密群 |
| `mute_chat` | 静音一个聊天的通知 |
| `resolve_username` | 把用户名解析为用户或聊天的 ID |
| `search_public_chats` | 按用户名或标题搜索公开聊天、频道或机器人 |
| `subscribe_public_channel` | 按用户名或 ID 订阅（加入）公开频道或超级群 |
| `unarchive_chat` | 取消归档一个聊天 |
| `unmute_chat` | 取消一个聊天的静音 |

</details>

<details>
<summary>媒体 (9)</summary>

| 工具 | 说明 |
|------|------|
| `download_media` | 把聊天消息中的媒体下载到本地文件，默认存临时文件 |
| `get_gif_search` | 通过 @gif 内联机器人按关键词搜 GIF，返回文档列表 |
| `get_media_info` | 获取消息中媒体的信息（类型、ID、名称、MIME、大小） |
| `get_photo_sheet` | 批量下载某用户的头像缩略图（最多 12 张）并返回路径 |
| `get_sticker_sets` | 获取已安装的所有贴纸包（标题），注意标题不可信 |
| `inspect_document` | 在内存中检查消息中的文档/图片，返回分析结果 |
| `list_photos` | 把某用户的照片索引为文本，不传输图片 |
| `open_photo` | 下载某用户的一张照片（头像用 photo_id，群头像用 message_id） |
| `upload_file` | 上传本地文件到 Telegram，返回元数据（名称、大小、md5） |

</details>

<details>
<summary>联系人 (16)</summary>

| 工具 | 说明 |
|------|------|
| `add_contact` | 给你的 Telegram 账号添加新联系人，需提供手机号或用户名 |
| `block_user` | 按用户 ID 或用户名拉黑用户 |
| `delete_contact` | 按用户 ID 或用户名删除联系人 |
| `delete_contact_alias` | 删除一个已记住的别名（仅本地），此构建不支持 |
| `export_contacts` | 把所有联系人导出为 JSON 字符串 |
| `get_blocked_users` | 获取黑名单用户列表 |
| `get_contact_chats` | 列出与指定联系人相关的所有聊天（私聊+共同群/频道） |
| `get_contact_ids` | 获取你 Telegram 账号中的所有联系人 ID |
| `get_direct_chat_by_contact` | 按姓名、用户名或手机号找到与指定联系人的私聊 |
| `get_last_interaction` | 获取与某联系人最近的消息往来 |
| `import_contacts` | 批量导入联系人，每个含 phone、first_name、last_name |
| `list_contact_aliases` | 列出已记住的联系人别名（仅本地），此构建不支持 |
| `list_contacts` | 列出你 Telegram 账号中的所有联系人 |
| `search_contacts` | 按姓名、用户名或手机号搜索联系人 |
| `set_contact_alias` | 记住你对某人的称呼（仅本地别名），此构建不支持 |
| `unblock_user` | 按用户 ID 或用户名取消拉黑 |

</details>

<details>
<summary>资料 (11)</summary>

| 工具 | 说明 |
|------|------|
| `delete_profile_photo` | 删除你当前的头像 |
| `get_bot_info` | 按用户名获取机器人信息，注意名称/简介字段不可信 |
| `get_full_user` | 获取 Telegram 用户的完整资料：简介、频道链接、生日等 |
| `get_me` | 获取你自己的 Telegram 用户信息 |
| `get_privacy_settings` | 获取你的最后上线时间隐私设置 |
| `get_user_photos` | 获取用户的头像 ID（最新在前） |
| `get_user_status` | 获取用户的在线/最后上线状态 |
| `set_bot_commands` | 为你拥有的机器人设置命令，仅登录账号是该机器人时可用 |
| `set_privacy_settings` | 设置隐私规则，key 可为 status（最后上线）、phone 或 profile_photo |
| `set_profile_photo` | 用本地图片文件设置新的头像 |
| `update_profile` | 更新你的资料（名称、简介），只改传入的字段 |

</details>

<details>
<summary>文件夹 (10)</summary>

| 工具 | 说明 |
|------|------|
| `add_chat_to_folder` | 把聊天（纯数字 ID 或 @username）加到已有文件夹，幂等 |
| `create_folder` | 创建新的对话文件夹，chat_ids 可用纯数字 ID 或 @username |
| `delete_folder` | 删除文件夹，里面的聊天保留，只删文件夹本身 |
| `get_folder` | 获取指定文件夹的详细信息，含包含/排除项 |
| `get_folder_limits` | 读取考虑 Premium 的文件夹、上限等配置限制 |
| `get_folder_snapshot` | 读取完整的文件夹定义和排序，不含聊天内容 |
| `list_folders` | 获取所有对话文件夹（过滤器），含 ID、名称和 emoji |
| `remove_chat_from_folder` | 把聊天（纯数字 ID 或 @username）从文件夹移除，幂等 |
| `reorder_folders` | 调整文件夹顺序，folder_ids 必须列出全部现有文件夹 ID |
| `update_folder` | 按 ID 更新私有文件夹，未传的字段保持原样 |

</details>

<details>
<summary>账号 (1)</summary>

| 工具 | 说明 |
|------|------|
| `list_accounts` | 列出已配置的 Telegram 账号及资料（名称、手机号、状态） |

</details>

<details>
<summary>事件 (5)</summary>

| 工具 | 说明 |
|------|------|
| `disable_incoming_feed` | 关闭消息事件流（停止写入事件文件） |
| `enable_incoming_feed` | 启用回调模式：后台任务把每批收敛后的消息追加写入 |
| `incoming_feed_status` | 报告消息事件流是否启用、文件路径和监听状态 |
| `wait_for_new_message` | 阻塞直到收到非机器人用户的私聊新消息，然后返回 |
| `wait_for_settled_message` | 事件驱动的防抖等待，直到某私聊收到收敛后的消息 |

</details>

<details>
<summary>兼容接口 (4)</summary>

| 工具 | 说明 |
|------|------|
| `get_messages_context` | 获取指定消息前后的上下文消息 |
| `get_profile` | 获取用户、群组或频道的资料信息 |
| `join_channel` | 按 @username 加入公开频道 |
| `search_in_channel` | 在指定频道或群组内按关键词搜索消息 |

</details>

<details>
<summary>其他 (2)</summary>

| 工具 | 说明 |
|------|------|
| `get_send_as` | 列出聊天中允许的代发身份（peer ID、名称等） |
| `send_contact` | 向聊天发送一张联系人名片 |

</details>

</details>

## 快速开始

> 🤖 **让 AI 帮你装**：复制下面这句发给你的 AI 助手：
> ```
> 阅读 https://github.com/zxsos/Telegram-mcp/blob/master/AGENT.md，帮我安装 Telegram MCP。
> ```

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

# 使用 SOCKS5 代理（可选）
./target/release/telegram-mcp --proxy socks5://user:pass@host:port --auth
# 或设置 TG_PROXY 环境变量：
# TG_PROXY=socks5://user:pass@host:port ./target/release/telegram-mcp --auth

# 4. 接入 MCP 客户端
```

Claude Code（命令行）：
```bash
claude mcp add --transport stdio telegram -- /absolute/path/to/telegram-mcp
```

**Claude Desktop / Cursor / Windsurf / Cline** —— 加到配置文件 JSON：
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
配置文件位置：
- Claude Desktop（macOS）：`~/Library/Application Support/Claude/claude_desktop_config.json`
- Claude Desktop（Windows）：`%APPDATA%\Claude\claude_desktop_config.json`
- Cursor：`~/.cursor/mcp.json`（全局）或 `.cursor/mcp.json`（项目内）
- Windsurf：`~/.codeium/windsurf/mcp_config.json`

**VS Code**（`.vscode/mcp.json`）——注意格式不同：
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

**Zed**（`~/.config/zed/settings.json`）：
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

**OpenAI Codex**（`~/.codex/config.toml`）：
```toml
[mcp_servers.telegram]
command = "/absolute/path/to/telegram-mcp"

[mcp_servers.telegram.env]
TG_API_ID = "your_api_id"
TG_API_HASH = "your_api_hash"
```
或用命令行：`codex mcp add telegram --command /absolute/path/to/telegram-mcp`

**腾讯 WorkBuddy**（`~/.workbuddy/.mcp.json`）：
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

**腾讯 CodeBuddy CLI**（`~/.codebuddy/.mcp.json`）：
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

> 提示：`command` 请写二进制文件的绝对路径（例如 `/home/user/telegram-mcp/target/release/telegram-mcp`）。图形界面客户端可能读不到你 shell 的 PATH。

### 使用示例

连接成功后，直接跟 AI 对话：

- "列出我最近的聊天"
- "看看过去一小时的未读消息"
- "给 @username 发个 '你好'"
- "在我的群里搜索关于'开会'的消息"
- "下载收藏夹里最新的照片"

## License

Apache-2.0
