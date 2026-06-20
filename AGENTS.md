# AGENTS.md

禁止提交 `.env`、凭据、私钥。提交前 `git diff --staged` 自查。

## 仓库级 Skill

本仓库包含项目级 skill：`.agents/skills/codex-browser/SKILL.md`

该 skill 是 LLM agent 使用全部 36 个 MCP 工具的操作手册，包含工具分组速查、常用工作流、工具选择原则。支持项目级 skill 的 MCP 客户端会自动加载。

## MCP 工具设计规范

新增 MCP 工具遵循以下模式：

### 1. Browser 层（`src/browser.rs`）
- 每个 CDP 操作封装为一个 `pub async fn`，接受 `&Client` + 参数
- CDP 响应解析用私有函数，返回 `Result<T>`
- 使用 `#[derive(Deserialize)]` 私有结构体解析 CDP 响应
- `execute_cdp_generic()` 是通用 CDP 入口

### 2. MCP 层（`src/mcp/` 目录）
- `types.rs`：`ToolHandler` 枚举新增 variant（当前 36 个变体）
- `handlers.rs`：`handle_tool_call` 新增 match arm + `handle_*` 方法
- `schema.rs`：`registered_tools()` 新增工具定义
- `profiles.rs`：如需加入 profile，更新 `BASIC_TOOLS` / `NETWORK_TOOLS` 数组

### 3. 安全层（`src/security.rs`）
- 新增文件操作需经过 `validate_file_path()` 路径穿越防护
- 新增 URL 参数需经过 `validate_url()` scheme 检查

### 4. 诊断（`src/doctor.rs`）
- `run_diagnostics()` — 独立于 MCP Server 的 pipe 探活逻辑

### 5. 测试
- **Extractor 测试**（`src/mcp/types.rs`）：`required_str`、`required_string_vec` 等
- **Schema 测试**（`src/mcp/schema.rs`）：验证工具 required 字段、name order、type=object
- **E2E 测试**（`tests/cdp_tools_e2e.rs`）：`client_server_pair()` + mock CDP server
- **Parity 测试**（`tests/mcp_parity.rs`）：跨版本工具名一致性

### 6. 工具数量
- 当前：36 个 MCP 工具
- 新增工具命名：`codex_<domain>_<action>`，group tag 放描述开头

## 构建

```bash
cargo check --locked              # 快速检查
cargo test --locked                # 全量测试
cargo clippy --locked -- -D warnings  # lint
cargo build --locked --release     # 发布构建 → target/release/codex-browser-bridge.exe
```

Release 时确保 `Cargo.toml` 和 `npm/package.json` 版本号与 tag 一致，`CHANGELOG.md` 有对应段。

## 源码结构

```
src/
  main.rs       入口（clap CLI，--mode --profile --pipe --upload-base）
  lib.rs        模块声明
  mcp/
    mod.rs      Server 结构体, run_stdio, JSON-RPC 分发
    types.rs    ToolHandler, Tool, Content, arg extractors, 响应构建
    schema.rs   registered_tools(), 工具注册
    handlers.rs handle_tool_call + 36 个 handle_* 方法
    profiles.rs ToolProfile (basic/network/full)
  browser.rs    CDP + 浏览器操作（list_tabs, navigate, screenshot, click, etc.）
  client.rs     Named pipe 传输, sticky attach, execute_cdp
  security.rs   URL scheme 验证 + 文件路径穿越防护
  doctor.rs     Pipe 诊断（枚举 + 探活 + 延迟测量）
  cli.rs        交互式调试 REPL
  discovery.rs  Named pipe 自动发现
  protocol.rs   Length-prefixed JSON-RPC 帧编解码
  error.rs      BridgeError 枚举
  pipe.rs       Windows named pipe 连接
  logging.rs    日志初始化
```

## 分支策略

- `main` — 稳定分支，CI 通过才合并
- feature 分支 — `feat/<name>`
- Release tag — `v*` 触发 GitHub Release + npm publish

## 安全红线

- 不在仓库中存放 `.env`、私钥、token
- GitHub Actions 使用 `${{ secrets.* }}`
- CDP 参数不硬编码敏感 URL
- 测试数据使用示例域名（`example.com`）
- 文件操作经过 `security::validate_file_path` 路径穿越检查
- Cookie 值默认脱敏
- CDP allowlist 阻止 Browser/Debugger/Target/Emulation/Security/Tracing 域
