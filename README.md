# Telegram MCP

轻量 Telegram MCP 服务器（Rust），用你的 Telegram 账号登录，可读你加入的所有群/频道（含私密群）。

## 工具（22）

### 基础
| 工具 | 说明 |
|------|------|
| `list_chats` | 列出所有对话（群/频道/私聊），返回 ID |
| `get_messages` | 按 ID 读指定聊天的最新消息 |
| `search_in_channel` | 在指定频道内搜关键词 |
| `get_messages_context` | 读某条消息的上下文 |
| `get_profile` | 查用户/频道资料 |
| `join_channel` | 加入公开频道 |

### 账号
| 工具 | 说明 |
|------|------|
| `list_accounts` | 查看当前登录账号信息 |

### 文件夹
| 工具 | 说明 |
|------|------|
| `list_folders` | 列出所有文件夹 |
| `get_folder` | 查看文件夹详情 |
| `get_folder_snapshot` | 文件夹快照（含顺序、版本） |
| `get_folder_limits` | 查看文件夹数量限制 |
| `create_folder` | 创建文件夹 |
| `update_folder` | 更新文件夹 |
| `delete_folder` | 删除文件夹 |
| `add_chat_to_folder` | 加对话到文件夹 |
| `remove_chat_from_folder` | 从文件夹移除对话 |
| `reorder_folders` | 文件夹排序 |

### 事件（需更新流支持，暂不可用）
| 工具 | 说明 |
|------|------|
| `wait_for_new_message` | 等待新私信 |
| `wait_for_settled_message` | 等待消息稳定 |
| `enable_incoming_feed` | 启用消息流 |
| `disable_incoming_feed` | 停用消息流 |
| `incoming_feed_status` | 消息流状态 |

*更多模块（消息、群组、联系人、媒体等）正在添加中。*

## 快速开始

```bash
# 1. 准备 Telegram API 凭证（https://my.telegram.org/apps）
# 2. 编译
cargo build --release

# 3. 登录（只需一次，需手机验证码）
./target/release/telegram-mcp --auth

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
