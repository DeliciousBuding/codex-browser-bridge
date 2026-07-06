# AGENTS.md

禁止提交 `.env`、凭据、私钥。提交前 `git diff --staged` 自查。

## 仓库级 Skill

本仓库包含项目级 skill：`skills/codex-browser/SKILL.md`

该 skill 是 LLM agent 使用全部 52 个 MCP 工具的操作手册，包含工具分组速查、常用工作流、工具选择原则。支持项目级 skill 的 MCP 客户端会自动加载。

## MCP 工具设计规范

新增 MCP 工具遵循以下模式：

### 1. Browser 层（`src/browser.rs`）
- 每个 CDP 操作封装为一个 `pub async fn`，接受 `&Client` + 参数
- CDP 响应解析用私有函数，返回 `Result<T>`
- 使用 `#[derive(Deserialize)]` 私有结构体解析 CDP 响应
- `execute_cdp_generic()` 是显式低风险方法 allowlist 的 raw CDP 入口；不要用域名前缀放宽，高风险能力必须走专用受控工具

### 2. MCP 层（`src/mcp/` 目录）
- `types.rs`：`ToolHandler` 枚举新增 variant（当前 52 个工具）
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
- **集成测试**（`tests/browser_api.rs`）：公开 browser/helper 行为与安全边界
- **Reconnect/mock 测试**（`src/client.rs`，`cfg(not(windows))`）：`tokio::io::duplex()` mock pipe
- **Harness 计划**：新的真实/模拟 E2E 应放在 `tests/`，不要使用 `#[path]` 嵌入源码文件

### 6. 工具数量
- 当前：52 个 MCP 工具
- 新增工具命名：`codex_<domain>_<action>`，group tag 放描述开头

## 构建

```bash
cargo check --locked              # 快速检查
cargo test --locked                # 全量测试
cargo clippy --locked -- -D warnings  # lint
cargo build --locked --release     # 发布构建 → target/release/codex-browser-bridge.exe
npm --prefix npm test              # npm installer/package helper 测试
.\scripts\live-e2e.ps1             # 可选真实 Codex Desktop + Chrome 冒烟测试
```

Release 时遵循 `docs/release-process.md`：确保 `Cargo.toml` 和 `npm/package.json` 版本号与 tag 一致，`CHANGELOG.md` 有对应段，npm 包 dry-run 包含 `skills/codex-browser/SKILL.md`。npm 发布走 Trusted Publishing/OIDC，不在 GitHub secrets 中放 npm write token。

## 源码结构

```
src/
  main.rs       入口（clap CLI，--mode --profile --pipe --upload-base --max-text-bytes --max-image-bytes）
  lib.rs        模块声明
  mcp/
    mod.rs      Server 结构体, run_stdio, JSON-RPC 分发
    types.rs    ToolHandler, Tool, Content, arg extractors, 响应构建
    schema.rs   registered_tools(), 工具注册
    handlers.rs handle_tool_call + 52 个 handle_* 方法
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
- Release tag — `vX.Y.Z` 触发 GitHub Release + npm publish

## 安全红线

- 不在仓库中存放 `.env`、私钥、token
- GitHub Actions 使用 `${{ secrets.* }}`
- CDP 参数不硬编码敏感 URL
- 测试数据使用示例域名（`example.com`）
- 文件操作经过 `security::validate_file_path` 路径穿越检查
- Cookie 值默认脱敏
- URL 导航只允许 `http://` / `https://`
- CDP raw 入口只允许显式列出的低风险方法，阻止 Browser/Debugger/Target/Emulation/Security/Tracing 域，以及 navigation/cookie/screenshot/PDF/file upload/page-resource content/destructive storage 等敏感 raw CDP 方法
- MCP 输出统一经过 `Content` 层上限：`CODEX_BRIDGE_MAX_TEXT_BYTES` / `max_text_bytes` 默认 1 MiB，`CODEX_BRIDGE_MAX_IMAGE_BYTES` / `max_image_bytes` 默认 3 MiB，配置值最高 8 MiB；不要在单个 handler 里绕过该出口。
