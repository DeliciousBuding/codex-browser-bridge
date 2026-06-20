# AGENTS.md

禁止提交 `.env`、凭据、私钥。提交前 `git diff --staged` 自查。

## MCP 工具设计规范

新增 MCP 工具遵循以下模式：

### 1. Browser 层（`src/browser.rs`）
- 每个 CDP 操作封装为一个 `pub async fn`，接受 `&Client` + 参数
- CDP 响应解析用私有函数（`fn parse_*`），返回 `Result<T>`
- 使用 `#[derive(Deserialize)]` 私有结构体解析 CDP 响应
- `execute_cdp_generic()` 是通用 CDP 入口——不需要为每个 domain 写专用函数也可用

### 2. MCP 层（`src/mcp.rs`）
- `ToolHandler` 枚举新增 variant
- `handle_tool_call` 新增 match arm
- `registered_tools()` 新增工具定义（按字母序排列在现有工具后）
- 每个 handle 函数：解析参数 → 调用 browser 函数 → 返回 `Vec<Content>`

### 3. 测试
- **单元测试**（`src/browser.rs`）：Mock CDP 响应解析
- **Schema 测试**（`src/mcp.rs`）：验证 required 字段
- **E2E 测试**（`tests/cdp_tools_e2e.rs`）：`client_server_pair()` + `mock_cdp_server()` 模拟完整管道
- Parity 测试（`tests/mcp_parity.rs`）：工具名列表与 Go 版本对齐

### 4. 工具总数
- 当前：28 个 MCP 工具（24 原有 + 4 新增）
- 新增工具命名：`codex_<domain>_<action>`

## 构建

```bash
cargo check --locked          # 快速检查
cargo test --locked            # 全量测试
cargo build --locked --release # 发布构建 → target/release/codex-browser-bridge.exe
```

Release 时确保 `Cargo.toml` 和 `npm/package.json` 版本号与 tag 一致，`CHANGELOG.md` 有对应段。

## 分支策略

- `main` — 稳定分支，CI 通过才合并
- feature 分支 — `feat/<name>`
- Release tag — `v*` 触发 GitHub Release + npm publish

## 安全红线

- 不在仓库中存放 `.env`、私钥、token
- GitHub Actions 使用 `${{ secrets.* }}`
- CDP 参数不硬编码敏感 URL
- 测试数据使用示例域名（`example.com`）
