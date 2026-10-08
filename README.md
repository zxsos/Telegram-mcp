# Telegram MCP

轻量 Telegram MCP 服务器（Rust），用你的 Telegram 账号登录，可读你加入的所有群/频道（含私密群）。

## 工具（90）

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

### 媒体
| 工具 | 说明 |
|------|------|
| `send_file` | 发送文件（自动识别图片/文档） |
| `send_album` | 发送相册（2-10 张） |
| `download_media` | 下载媒体文件 |
| `send_voice` | 发送语音 |
| `upload_file` | 上传文件 |
| `get_media_info` | 查看媒体信息 |
| `get_sticker_sets` | 列出贴纸包 |
| `send_sticker` | 发送贴纸 |
| `get_gif_search` | 搜索 GIF |
| `send_gif` | 发送 GIF（暂不支持） |
| `list_photos` | 列出照片 |
| `open_photo` | 打开照片（返回路径） |
| `get_photo_sheet` | 照片缩略图列表 |
| `inspect_document` | 查看文档内容 |

### 联系人
| 工具 | 说明 |
|------|------|
| `list_contacts` | 列出所有联系人 |
| `search_contacts` | 搜索联系人 |
| `get_contact_ids` | 获取联系人 ID 列表 |
| `get_direct_chat_by_contact` | 按联系人找私聊 |
| `get_contact_chats` | 联系人的共同群聊 |
| `get_last_interaction` | 最近互动消息 |
| `add_contact` | 添加联系人 |
| `delete_contact` | 删除联系人 |
| `block_user` | 拉黑用户 |
| `unblock_user` | 解除拉黑 |
| `import_contacts` | 批量导入联系人 |
| `export_contacts` | 导出联系人 |
| `get_blocked_users` | 黑名单列表 |
| `send_contact` | 发送联系人名片 |

### 消息
| 工具 | 说明 |
|------|------|
| `send_message` | 发送消息 |
| `send_scheduled_message` | 发送定时消息 |
| `get_scheduled_messages` | 查看定时消息 |
| `delete_scheduled_message` | 删除定时消息 |
| `list_messages` | 列出消息（支持搜索/日期过滤） |
| `list_inline_buttons` | 列出消息的内联按钮 |
| `press_inline_button` | 点击内联按钮 |
| `transcribe_voice` | 语音转文字（暂不支持） |
| `get_message_context` | 读消息上下文 |
| `get_send_as` | 可用的发送身份 |
| `forward_message` | 转发单条消息 |
| `forward_messages` | 批量转发消息 |
| `edit_message` | 编辑消息 |
| `delete_message` | 删除消息 |
| `delete_chat_history` | 清空聊天记录 |
| `delete_messages_bulk` | 批量删除消息 |
| `pin_message` | 置顶消息 |

### 对话
| 工具 | 说明 |
|------|------|
| `get_chats` | 分页列出对话 |
| `get_chat` | 查看对话详情 |
| `get_full_chat` | 完整对话信息 |
| `search_public_chats` | 搜索公开群/频道 |
| `resolve_username` | 解析用户名 |
| `subscribe_public_channel` | 订阅公开频道 |
| `mute_chat` / `unmute_chat` | 静音/取消静音 |
| `archive_chat` / `unarchive_chat` | 归档/取消归档 |
| `list_topics` | 列出话题（论坛群） |
| `create_forum_topic` | 创建话题 |
| `edit_forum_topic` | 编辑话题 |
| `delete_forum_topic` | 删除话题 |
| `enable_forum_topics` | 启用话题功能 |
| `get_common_chats` | 共同群聊 |
| `get_message_read_by` | 消息已读情况 |
| `get_message_link` | 获取消息链接 |

*更多模块（群组等）正在添加中。*

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
