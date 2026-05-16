# codex-browser-bridge — 项目交接文档

> 最后更新: 2026-05-16
> 状态: **Pipe 连接验证通过，核心 API 可用，CDP 层受限**

---

## 这个项目要干什么

让 Claude Code（或任何 AI Agent）通过 Codex Desktop 的 Chrome 扩展基础设施，直接控制用户现有 Chrome/Edge 浏览器标签页。

核心链路：
```
AI Agent (MCP client)
  → codex-browser-bridge (本项目, Go 单文件)
    → Windows Named Pipe (\\.\pipe\codex-browser-use-*)
      → extension-host.exe (Codex 的 native messaging host)
        → Chrome Extension (hehggadaopoacecdllhhajmbjkdcmajg)
          → 控制 Chrome 标签页
```

## 为什么要做这个

Codex Desktop 的 `browser-client.mjs`（831KB minified）依赖 `import.meta.__codexNativePipe`，这是 Codex Desktop 自定义 ESM loader 注入的特权对象，Claude Code 的 Node.js 运行时无法获取。但我们逆向分析发现底层协议非常简单（标准 JSON-RPC 2.0 over length-prefixed frames），可以独立实现。

---

## 已完成的工作

### 1. 逆向分析 (100% 完成)

从 831KB minified `browser-client.mjs` 中提取出：

**Wire Protocol:**
- 4 字节 little-endian uint32 长度前缀 + JSON payload
- 标准 JSON-RPC 2.0 消息格式
- 完全相同于 Codex 开源代码中 `codex-rs/tui/src/ide_context/ipc.rs` 的帧协议

**关键源码位置 (minified 行号):**
- `da` class (JSON-RPC base): line 110 — `sendRequest` 使用 `{jsonrpc:"2.0", id, method, params}`
- `Kc` class (pipe transport): line ~3040 — `Tm()` 检查 `import.meta.__codexNativePipe`
- `Qc` class (session transport): line ~3070 — 注入 `session_id` + `turn_id`
- `setupBrowserRuntime` / `Ale`: line 2984 — 入口函数
- Pipe path: `Xv` function — `\\.\pipe\codex-browser-use` (Windows) / `/tmp/codex-browser-use` (Unix)
- Security bypass: `k8()` 检查 `x-codex-browser-use-security-mode: disabled-for-local-testing`

**Command 列表 (确认可用):**
| 分类 | 命令 |
|------|------|
| Tab 管理 | `create_tab`, `list_tabs`, `close_tab`, `selected_tab` |
| 导航 | `navigate_tab_url`, `navigate_tab_back`, `navigate_tab_forward`, `navigate_tab_reload` |
| Playwright | `playwright_dom_snapshot`, `playwright_screenshot`, `playwright_click`, `playwright_fill`, `playwright_evaluate`, `playwright_wait_for_load_state` |
| CUA (坐标) | `cua_click`, `cua_type`, `cua_keypress`, `cua_scroll`, `cua_move`, `cua_drag` |
| DOM CUA | `dom_cua_click`, `dom_cua_get_visible_dom`, `dom_cua_type`, `dom_cua_double_click` |
| 用户 Tab | `browser_user_open_tabs`, `browser_user_claim_tab`, `browser_user_history` |
| 会话 | `name_session`, `finalize_tabs` |
| 诊断 | `get_info`, `ping` |

### 2. 环境诊断 (100% 完成)

用户的机器状态 — 全部正常：
- Chrome v147.0.7727.138 已安装且运行中
- Extension 1.1.4_0 已安装、已启用
- Native host manifest 存在且路径正确
- Chrome/Edge 注册表键均存在
- extension-host.exe 存在
- 9 个 `codex-browser-use-*` named pipe 活跃

### 3. Go 项目搭建 (60% 完成)

**已编译通过，`bridge.exe` 已生成。Pipe 发现功能已验证。**

```
codex-browser-bridge/
├── cmd/bridge/main.go          # CLI 入口 — 三种模式: mcp, cli, discover
├── internal/
│   ├── protocol/protocol.go    # 帧编解码 + JSON-RPC 类型定义
│   ├── discovery/discovery.go  # 枚举 \\.\pipe\ 过滤 codex-browser-use-*
│   ├── client/
│   │   ├── client.go           # 连接管理 + 请求/响应关联 + 超时
│   │   ├── browser.go          # 高层 API: ListTabs, Navigate, DOMSnapshot 等
│   │   └── pipe_windows.go     # go-winio named pipe 连接
│   └── mcp/
│       └── server.go           # MCP server (stdio JSON-RPC) + 20+ tool 定义
├── go.mod
├── go.sum
└── bridge.exe                  # 已编译的二进制
```

**已验证的功能:**
- `bridge.exe -mode discover` → 成功列出 9 个活跃的 codex-browser-use pipes
- 编译通过，无错误
- 依赖: `github.com/Microsoft/go-winio v0.6.2` + `golang.org/x/sys`

---

## 未完成的工作 / 下一步

### ✅ 已完成 (2026-05-16)

1. **Pipe 连接调试** — 完成
   - go-winio `DialPipe` 成功连接 `codex-browser-use\*` pipes
   - 关键发现：wire protocol 方法名是 **camelCase**（`getInfo`、`getTabs`、`createTab`），不是 snake_case
   - `executeCdp` 需要 `{target: {tabId}}` 嵌套格式，且必须先调用 `attach`
   - 每次连接创建新 session，tab 不能跨 session 使用

2. **核心 API 验证** — 完成
   - `ping` → `"pong"` ✅
   - `getInfo` → Chrome extension 1.1.4 ✅
   - `getTabs` → session 内标签页列表 ✅
   - `createTab` → 创建新标签页 ✅
   - `getUserTabs` → 用户浏览器所有标签页 ✅（返回裸数组，id 可能是数字）
   - `claimUserTab` → 需要整数 tabId ✅
   - `nameSession` → 会话命名 ✅
   - `executeCdp` → `{target:{tabId}}` + `attach` ✅

3. **端到端测试** — 完成
   - `createTab → attach → executeCdp(Page.navigate) → getTabs` 全流程通过
   - 导航到 https://example.com 成功，标签页标题更新为 "Example Domain"

### 中优先级: MCP Server

- [ ] 测试 MCP stdio 模式（`bridge.exe -mode mcp`）
- [ ] MCP tool handler 使用正确的 camelCase 方法名
- [ ] 处理 `initialized` notification
- [ ] screenshot 返回 base64 PNG
- [ ] DOM snapshot 通过 `executeCdp` + `Runtime.evaluate` 实现

### 低优先级: 打磨

- [ ] `go install` 安装支持
- [ ] README.md
- [ ] 环境变量配置 (`CODEX_PIPE_NAME`, `CODEX_SESSION_TIMEOUT`)
- [ ] 日志级别控制 (`-log-level debug|info|warn|error`)
- [ ] 单元测试 (frame encode/decode)
- [ ] 非 Windows 平台的 stub 实现
- [ ] 多 pipe 时智能选择 extension vs iab vs cdp 后端

---

## 关键技术决策记录

| 决策 | 选择 | 原因 |
|------|------|------|
| 语言 | Go | 原生 Windows Named Pipe 支持 (go-winio)，编译为单个 exe，JSON 处理零依赖 |
| 不用 TS | 是 | Node.js 无法枚举 named pipes，需要 spawn PowerShell 子进程；依赖 Node 运行时 |
| 不用 Rust | 是 | 对这个桥接层来说太重，编译慢，迭代慢 |
| 不复用 browser-client.mjs | 是 | `import.meta.__codexNativePipe` 是 Codex Desktop 私有注入，无法绕过 |
| 安全绕过 | `x-codex-browser-use-security-mode: disabled-for-local-testing` | 从 minified 源码中发现的开发测试用后门头 |

---

## 参考资料

**Codex Desktop 源码（缓存路径，只读）:**
- `C:\Users\Ding\.codex\.tmp\bundled-marketplaces\openai-bundled\plugins\chrome\scripts\browser-client.mjs` — 831KB minified，包含完整协议实现
- `C:\Users\Ding\.codex\.tmp\bundled-marketplaces\openai-bundled\plugins\chrome\skills\chrome\SKILL.md` — 使用文档

**Codex 开源仓库（参考）:**
- `D:\Code\Projects\codex-source\codex-rs\tui\src\ide_context\ipc.rs` — 帧协议参考实现 (Rust)
- `D:\Code\Projects\codex-source\codex-rs\tui\src\ide_context\windows_pipe.rs` — Windows pipe 参考

**Native Host 配置:**
- Manifest: `C:\Users\Ding\AppData\Local\OpenAI\extension\com.openai.codexextension.json`
- Extension ID: `hehggadaopoacecdllhhajmbjkdcmajg`
- Host name: `com.openai.codexextension`
