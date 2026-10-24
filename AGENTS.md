# AGENTS.md

禁止提交 `.env`、凭据、私钥。提交前 `git diff --staged` 自查。

## 仓库级 Skill

本仓库包含项目级 skill bundle：`skills/codex-browser/SKILL.md` + `references/`

`SKILL.md` 只负责路由和工作流，任务细节按需读取一个 reference。支持项目级 skill 的客户端会自动发现；Claude Code 可通过仓库内 marketplace 把 skill 与 MCP server 作为同一个插件安装。

## 插件分发层

插件是分发层，MCP server 是执行层，两者不重复实现：

- `.claude-plugin/plugin.json` + `.claude-plugin/marketplace.json`：Claude Code 插件，复用根 `mcp.json`
- `mcp.json` 通过 `npx` 启动固定版本的 npm 包；插件本身不分发 Windows 二进制。全局 npm 安装是可选的快速路径，手工 MCP 示例仍使用它。

Codex / ChatGPT desktop 已经能通过现有浏览器扩展直接控制浏览器，不再把 bridge 包装成 Codex 插件，也不安装这个重复入口。bridge 继续作为通用 MCP server，供 Claude Code、Cursor、OpenClaw、Hermes Agent 等非 Codex 客户端复用同一登录态。

Claude 插件默认使用 `full` profile，因为 Claude Code 会通过 tool search 延迟加载 tool schema；只有 `codex_doctor` 使用 `anthropic/alwaysLoad=true`。不支持 tool search 的手工 MCP 配置仍推荐 `basic`。不要自造第二套工具路由器，也不要为节省 token 删除专用工具。

## MCP 工具设计规范

新增 MCP 工具遵循以下模式：

### 1. Browser 层（`src/browser.rs`）
- 每个 CDP 操作封装为一个 `pub async fn`，接受 `&Client` + 参数
- CDP 响应解析用私有函数，返回 `Result<T>`
- 使用 `#[derive(Deserialize)]` 私有结构体解析 CDP 响应
- `execute_cdp_generic()` 是显式低风险方法 allowlist 的 raw CDP 入口；不要用域名前缀放宽，高风险能力必须走专用受控工具

### 2. MCP 层（`src/mcp/` 目录）
- `types.rs`：`ToolHandler` 枚举新增 variant（当前 66 个工具）
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
- **Reconnect/mock 测试**（`src/client.rs`，`cfg(test)`）：跨平台 `tokio::io::duplex()` mock pipe
- **Harness 计划**：新的真实/模拟 E2E 应放在 `tests/`，不要使用 `#[path]` 嵌入源码文件
- Mock pipe / E2E harness 等待必须 bounded：正向 pipe read 和 spawned task join 用明确 timeout，负向“不得发 pipe 请求”断言用短 grace window；live E2E 的 doctor preflight 和 cleanup MCP 调用也必须有 timeout。

### 6. 工具数量
- 当前：66 个 MCP 工具；profile 为 basic=46、network=65、full=66
- 新增工具命名：`codex_<domain>_<action>`，group tag 放描述开头

## 构建

```bash
cargo check --locked              # 快速检查
cargo test --locked                # 全量测试
cargo clippy --locked -- -D warnings  # lint
cargo build --locked --release     # 发布构建 → target/release/codex-browser-bridge.exe
npm --prefix npm test              # npm installer/package helper 测试
.\scripts\live-e2e.ps1             # 可选真实 ChatGPT 桌面应用 + Chromium 浏览器冒烟测试
```

Release 时遵循 `docs/release-process.md`：确保 `Cargo.toml`、`npm/package.json`、`.claude-plugin/plugin.json` 的版本号与 tag 一致，`CHANGELOG.md` 有对应段，npm 包 dry-run 包含完整的 `skills/codex-browser/` bundle 和 `examples/` 多客户端模板。npm 发布走 Trusted Publishing/OIDC，不在 GitHub secrets 中放 npm write token。

发布前运行 `node scripts/check-plugin-surface.js` 与 `claude plugin validate --strict .`。前者在 CI 中验证 Claude manifest、唯一 MCP 配置和 marketplace 不漂移，并阻止 Codex 插件分发文件重新进入仓库。

`mcp.json` 的 npm 包版本必须与 `.claude-plugin/plugin.json`、`npm/package.json` 和 release tag 一致；`check-plugin-surface.js` 会拒绝漂移。

GitHub workflow 中的外部 `uses:` 必须固定到完整 commit SHA，并保留相邻版本注释；修改 workflow 后运行 `node scripts/check-actions-pinned.js`。

Release/npm 发布自动化必须保持 Trusted Publishing 合同：`release.yml` 使用 GitHub-hosted runner、`id-token: write`、npm >= 11.5.1、无 `NODE_AUTH_TOKEN`、`actions/setup-node` 禁用 `package-manager-cache`，并在文档中记录 npm trusted publisher 的 `release.yml` / `npm publish` 配置。修改 release workflow、`npm/package.json` 或发布文档后运行 `node scripts/check-release-contract.js`。

CI/release job 必须设置 `timeout-minutes`，长耗时测试步骤（Rust tests、coverage、live E2E timeout harness）也要有步骤级 timeout，避免 harness 回归占满 GitHub runner。

工具、profile、README、skill、examples 或 npm package 清单变更后运行 `node scripts/check-agent-surface.js`，防止 agent 面文档和实际 Rust 工具注册 drift。

## 源码结构

```
src/
  main.rs       入口（clap CLI，--mode --profile --pipe --upload-base --max-text-bytes --max-image-bytes）
  lib.rs        模块声明
  mcp/
    mod.rs      Server 结构体, run_stdio, JSON-RPC 分发（双纪元 era 检测入口）
    lifecycle.rs MCP 版本协商 + 双纪元生命周期（legacy initialize / 2026-07-28 stateless、server/discover、resultType/缓存提示、-32022）
    types.rs    ToolHandler, Tool, ToolHints(annotations), Content, arg extractors, 响应构建
    schema.rs   registered_tools(), 工具注册
    handlers.rs handle_tool_call + 66 个工具 handler
    profiles.rs ToolProfile (basic/network/full)
  browser.rs    CDP + 浏览器操作（list_tabs, navigate, screenshot, click, etc.）
  client.rs     Named pipe 传输, sticky attach, execute_cdp
  security.rs   URL scheme 验证 + 文件路径穿越防护
  doctor.rs     Pipe 诊断（枚举 + 探活 + 延迟测量）
  registry.rs   浏览器实例清单、身份识别与 fail-closed 选择
  cli.rs        tools/call/batch 命令和交互式调试 REPL
  discovery.rs  Windows named pipe / Linux Unix socket 自动发现
  protocol.rs   Length-prefixed JSON-RPC 帧编解码
  error.rs      BridgeError 枚举
  pipe.rs       Windows named pipe / Unix socket 连接
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
- CDP raw 入口只允许显式列出的诊断方法，阻止 Browser/Debugger/Target/Emulation/Security/Tracing 域，以及 navigation/cookie/screenshot/PDF/file upload/event-producing enable/arbitrary Runtime JS/page-resource content/destructive storage 等敏感 raw CDP 方法
- MCP 输出统一经过 `Content` 层上限：`CODEX_BRIDGE_MAX_TEXT_BYTES` / `max_text_bytes` 默认 1 MiB，`CODEX_BRIDGE_MAX_IMAGE_BYTES` / `max_image_bytes` 默认 3 MiB，配置值最高 8 MiB；不要在单个 handler 里绕过该出口。
