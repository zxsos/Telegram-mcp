# AGENT.md — AI Agent 安装指南

你是 AI 编程助手。用户让你安装 **Telegram MCP**（一个让 AI 读写 Telegram 的 MCP 服务器）。请按以下步骤操作。

## 1. 克隆仓库

```bash
git clone https://github.com/zxsos/Telegram-mcp.git
cd Telegram-mcp
```

## 2. 编译

需要 Rust 工具链（1.70+）：

```bash
cargo build --release
```

二进制文件位置：`./target/release/telegram-mcp`

## 3. 获取 Telegram API 凭证

让用户去 https://my.telegram.org 获取：
- `API ID`（数字）
- `API Hash`（字符串）

## 4. 登录 Telegram（需用户配合）

```bash
./target/release/telegram-mcp --auth
```

流程：
1. 输入 API ID 和 API Hash
2. 输入手机号（国际格式，如 +8613800000000）
3. Telegram 会发验证码到用户手机/App，让用户告诉你验证码，输入
4. 如果开了 2FA，输入两步验证密码（或设 `TG_2FA_PASSWORD` 环境变量）

登录成功后，session 保存在 `~/.telegram-mcp/`，下次不用再登录。

**非交互环境**：可设环境变量 `TG_CODE`（验证码）、`TG_2FA_PASSWORD`（2FA 密码），或把验证码写入 `/tmp/telegram_mcp_code`（程序会等待 5 分钟）。

## 5. 配置 MCP 客户端

### Claude Code
```bash
claude mcp add --transport stdio telegram -- /absolute/path/to/Telegram-mcp/target/release/telegram-mcp
```

### 通用 JSON（Claude Desktop / Cursor / Windsurf / Cline / WorkBuddy / CodeBuddy）
```json
{
  "mcpServers": {
    "telegram": {
      "command": "/absolute/path/to/Telegram-mcp/target/release/telegram-mcp",
      "env": {
        "TG_API_ID": "用户提供的",
        "TG_API_HASH": "用户提供的"
      }
    }
  }
}
```

### VS Code（`.vscode/mcp.json`）
```json
{
  "servers": {
    "telegram": {
      "type": "stdio",
      "command": "/absolute/path/to/Telegram-mcp/target/release/telegram-mcp"
    }
  }
}
```

### OpenAI Codex（`~/.codex/config.toml`）
```toml
[mcp_servers.telegram]
command = "/absolute/path/to/Telegram-mcp/target/release/telegram-mcp"

[mcp_servers.telegram.env]
TG_API_ID = "用户提供的"
TG_API_HASH = "用户提供的"
```

### Zed（`~/.config/zed/settings.json`）
```json
{
  "context_servers": {
    "telegram": {
      "command": {
        "path": "/absolute/path/to/Telegram-mcp/target/release/telegram-mcp",
        "args": []
      }
    }
  }
}
```

**注意**：`command` 必须用绝对路径。先跑 `pwd` 确认仓库位置，再拼接 `/target/release/telegram-mcp`。

## 6. 验证

让 AI 试着调用：
- `list_chats` — 列出聊天列表
- `get_me` — 获取当前登录用户信息

如果返回正常数据，说明安装成功。

## 7. 可选：SOCKS5 代理

如果网络受限，加 `--proxy` 参数：
```bash
./target/release/telegram-mcp --proxy socks5://user:pass@host:port
```
或设环境变量 `TG_PROXY=socks5://user:pass@host:port`。

## 故障排查

- **编译失败**：检查 Rust 版本 `rustc --version`，需要 1.70+
- **登录失败**：确认 API ID/Hash 正确，手机号格式带国际区号
- **MCP 连不上**：检查 `command` 路径是否为绝对路径，二进制是否有执行权限
- **工具返回错误**：先跑 `--auth` 重新登录，session 可能过期
