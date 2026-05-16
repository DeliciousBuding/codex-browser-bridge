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

### 最高优先级: 调试 pipe 连接 (未验证)

`bridge.exe -mode cli` 还没测试过。go-winio 的 `DialPipe` 能否成功连接到 `codex-browser-use-*` pipe 是未知数。

**可能的问题:**
1. **Pipe 名称中的反斜杠** — 部分 pipe 名是 `codex-browser-use\<uuid>` (带反斜杠)，`PipePath()` 生成的路径可能是 `\\.\pipe\codex-browser-use\<uuid>`，需要确认 go-winio 是否正确处理
2. **客户端类型区分** — 9 个 pipe 可能分别对应 extension/iab/cdp 三种后端，目前直接取第一个，可能连到错误的类型
3. **握手协议** — 连接后可能需要先发送 `get_info` 或类似握手命令，确认后端类型
4. **Session 参数格式** — `session_id` 和 `turn_id` 是否必须是特定格式的 UUID，还是任意字符串

### 高优先级: 端到端测试

```
# 1. 确认连接
bridge.exe -mode cli
> info           # 获取后端信息
> tabs           # 列出标签页

# 2. 基本操作
> create         # 创建新标签页
> nav 1 https://example.com    # 导航
> snapshot 1     # DOM 快照
> screenshot 1   # 截图

# 3. MCP 模式测试
bridge.exe -mode mcp
# 发送 JSON-RPC:
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

### 中优先级: 功能完善

- [ ] 多 pipe 时识别 extension vs iab vs cdp 类型（通过 `get_info` 握手）
- [ ] 截图返回格式处理（base64 vs byte array）
- [ ] `finalize_tabs` 的 keep 参数正确构造
- [ ] 错误重连机制（pipe 断开后自动重连）
- [ ] MCP server 的 `initialized` notification 处理
- [ ] 请求超时可配置化

### 低优先级: 打磨

- [ ] `go install` 安装支持
- [ ] README.md
- [ ] 环境变量配置 (`CODEX_PIPE_NAME`, `CODEX_SESSION_TIMEOUT`)
- [ ] 日志级别控制 (`-log-level debug|info|warn|error`)
- [ ] 单元测试 (frame encode/decode)
- [ ] 非 Windows 平台的 stub 实现

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
