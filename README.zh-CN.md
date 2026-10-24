<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    让 Claude Code 和其他 MCP Agent 通过 ChatGPT 桌面应用的浏览器扩展控制你现有的浏览器配置。
    <br>64 个 MCP 工具。纯 Rust。单文件二进制。
  </p>
</p>

<p align="center">
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License">
  </a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/releases">
    <img src="https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Latest Release">
  </a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square" alt="CI">
  </a>
  <a href="https://codecov.io/gh/DeliciousBuding/codex-browser-bridge">
    <img src="https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Coverage">
  </a>
  <a href="README.md">English</a>
</p>

---

## 它能做什么

`codex-browser-bridge` 把你本机的 **ChatGPT 桌面应用 + 浏览器扩展** 变成一个任何 agent 都能控制的 MCP 服务器。

无需复制浏览器配置。无需 WebDriver。无需远程配置。它发现本机已存在的
`codex-browser-use` 本地传输，识别每个实时浏览器扩展实例，使用相同的
JSON-RPC 协议，暴露 64 个 MCP 工具用于浏览器自动化。

本文描述 v1.12.0：64 个 MCP 工具，Windows 与 Linux GNU 包均由同一份带 tag 的
源码构建。

项目目标是让非 Codex agent 获得完整浏览器能力。具体方法与语义覆盖见
[能力对齐表](docs/official-parity.md)；工具数量或新增 CLI 入口都不代表已完整对齐。
应用私有服务、locator 等剩余差异在表中单独记录。

> **命名说明：** OpenAI 现在把这一入口称为 **ChatGPT 浏览器扩展**。底层 `codex-browser-use` pipe 和 JSON-RPC 方法名仍使用 Codex 时期标识，因此本项目继续保留 `codex_*` 工具名以维持兼容性。

**你的 Agent 可以：**

- 打开、关闭、切换浏览器标签页
- 导航页面、前进后退、等待加载
- 截取视口截图（PNG）
- 读取 DOM / 无障碍树（支持 ARIA role+name 搜索）
- 点击、输入、滚动——通过 CSS 选择器、坐标或无障碍节点 ID
- 在页面上下文中执行任意 JavaScript
- 上传文件到 `<input type=file>` 元素
- 处理 JavaScript 弹窗（alert / confirm / prompt）
- 读取和设置浏览器 Cookie
- 执行明确 allowlist 允许的 CDP 诊断命令（Chrome DevTools Protocol 逃生口）
- 通过 `codex_doctor` 自检诊断

适用于需要真实浏览器会话的场景——后台管理系统、已登录的 Web 应用、本地开发服务器、文档网站。

## 快速安装

Claude Code 插件（推荐）：

```bash
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

插件会在首次使用时通过 `npx` 拉取固定版本的 npm 包和对应平台二进制；全局 npm
安装是可选的。

独立 CLI 或手工 MCP 配置：

```bash
npm i -g @delicious233/codex-browser-bridge
```

或从 [GitHub Releases](https://github.com/DeliciousBuding/codex-browser-bridge/releases) 下载二进制。

**需要：** 本机存在支持的扩展传输 · npm/npx 路径需要 Node.js 18+ · 支持的 Chromium 浏览器 · ChatGPT 桌面应用/浏览器扩展服务

## 兼容性

最近一次实测：2026-10-24，ChatGPT 桌面应用（`OpenAI.Codex` 包）`26.1016.8120.0`，Edge 扩展 `1.26.1020.11845`。

| 层面 | 状态 |
|------|------|
| ChatGPT 桌面应用 | 当前频道已实测 |
| ChatGPT 浏览器扩展 | 当前频道已实测 |
| Chrome / Edge | 支持；本次审计实测 Edge |
| Brave / Opera / Vivaldi | 预计使用相同 Chromium/CDP 管线，但尚未实测 |
| MCP `2024-11-05` … `2025-11-25` | 支持 Legacy 生命周期 |
| MCP `2026-07-28` | 支持无状态生命周期与 discovery |
| Windows x64 | v1.12.0 通过 18 项合成标签页 Edge 冒烟；这不等于完整能力面验证 |
| Windows arm64 | 已发布二进制；本次发布未实测 |
| Linux GNU x64 | Ubuntu 24.04 / glibc 2.39 上通过全量原生 Rust/Node 门禁、release 构建和无浏览器 doctor；未做扩展 live 验证 |
| Linux GNU arm64 | arm64 QEMU 下通过 `cargo build --release --locked`；未做 arm64 运行时或扩展 live 验证 |
| macOS | Unix 代码可编译不代表运行时支持；无发布二进制或实测记录 |

Bridge 不会安装第二套浏览器扩展，也不会复制浏览器配置。一个 MCP session
只控制一个浏览器扩展实例，不会把多个浏览器 profile 的标签页合并成一个列表。

Linux 源码优先发现 `CODEX_BRIDGE_SOCKET_DIR`，否则依次尝试
`$XDG_RUNTIME_DIR/codex-browser-use` 和 `/tmp/codex-browser-use`。它会检查
socket 所有者、私有权限和 peer UID，并接受官方共享目录 `1777` 配合 socket
`0600` 的布局。Doctor 会报告 transport kind/path，同时保留兼容的 `pipe_*`
字段。Linux/WSL 进程需要本环境内的 socket，不会自动跨 OS 发现 Windows pipe。

### 浏览器实例识别

Chrome 和 Edge 共享 Chromium 自动化后端，但不是可互换实例；两个 Edge profile
也可能拥有相同的 `name` / `family`，却对应不同标签、Cookie、扩展和登录态。

已安装的 Codex `browser` 与 `chrome` 插件包同样复用同一套 Browser
client/service，manifest 只是产品包装；`browserFamily` 选择扩展与 profile
后端，不切换到另一套自动化引擎。

Bridge 使用 `getInfo.metadata.extensionInstanceId` 作为稳定的 `browser_id`，
pipe 名只作为传输诊断信息。不报告实例 ID 的旧扩展不会进入自动发现，只能用
`--pipe` 作为显式 legacy 传输覆盖。正常流程：

1. `codex_list_browsers` 列出所有实时实例。
2. 多实例时用 `codex_bind_browser` 绑定精确 `browser_id`。

只有一个实时实例时才允许隐式选择；多个实例会 fail closed。重连始终固定在
所选 `extensionInstanceId`，不会静默切到另一个 Chrome/Edge/profile。

进程级配置可使用 `--browser-family` / `--browser-instance`、
`CODEX_BRIDGE_BROWSER_FAMILY` / `CODEX_BRIDGE_BROWSER_INSTANCE`，或 TOML 的
`browser_family` / `browser_instance`。`--pipe` 是底层显式覆盖，不能与浏览器
选择器同时使用。

当前会话仍拥有标签时，重新绑定必须先 `codex_finalize`，或显式传入
`end_previous_turn: true`，让官方 turn cleanup 完成后再切换，避免遗留旧会话。
结束 turn 会保留已标记 handoff/deliverable 的标签。

## 30 秒接入 Claude Code

上面的命令会同时安装 Skill 和 MCP server 配置，不需要手工拼 MCP JSON。
Claude Code 2.1.292 及以上支持一条命令添加 marketplace 并安装插件。开发本地仓时
可用 `claude --plugin-dir .`。

首次启动 MCP 时，`npx` 需要几秒钟填充缓存。之后会复用缓存包；如果 PATH 上已有
全局安装的 `codex-browser-bridge`，也会直接复用。

插件默认暴露完整 64 个工具，因为 Claude Code 默认启用 MCP tool search：
启动时只加载 server instructions 和工具名，具体 schema 按需发现；
`codex_doctor` 是唯一标记 `alwaysLoad` 的 schema。随插件安装的 Skill 也只按
任务读取一个参考文件，不再一次加载完整工具手册。

不支持 tool search 的客户端，或需要手工接 MCP 时，先全局安装 npm 包，再在设置中添加：

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp"],
      "transport": "stdio",
      "env": {
        "CODEX_BRIDGE_PROFILE": "basic"
      }
    }
  }
}
```

重启 Claude Code，然后：

```
列出我打开的浏览器标签页。
打开 https://example.com 并截图。
找到登录按钮并点击它。
```

如果 MCP 客户端从 GUI 或定时任务启动，优先使用 npm `postinstall`
打印的绝对 `command` 路径配置。之后如需重新找路径，可从 npm 全局安装路径运行 CLI doctor：

```powershell
$bridge = Join-Path (npm prefix -g) "codex-browser-bridge.cmd"
& $bridge --mode doctor
```

把输出里的 `install.suggested_mcp_config.mcpServers.codex-browser.command`
填到客户端配置的 `command`。不支持 tool search 时可先用 45 个工具的 `basic` profile，需要时再切到 `network` 或 `full`。文件上传需要设置 `CODEX_BRIDGE_UPLOAD_BASE`；[examples/](examples/) 里有完整配置模板。

Codex 和 ChatGPT 桌面应用本身已经能直接控制 ChatGPT 浏览器扩展，不要把
这个 bridge 再安装成 Codex 插件；那会为同一个浏览器增加一条重复 MCP
路径。bridge 面向 Claude Code 以及其他需要复用现有浏览器会话的非
Codex MCP 客户端。

Cursor、OpenClaw、Hermes Agent 的配置见 [examples/](examples/)。npm 包内也会随包发布这些 `examples/` 模板。

> 💡 **Agent skill 已随插件安装。** 不支持插件的客户端可把 npm 包内
> `skills\codex-browser` 复制到 `%USERPROFILE%\.claude\skills\`，Git
> Bash/WSL 用户可复制到 `~/.claude/skills/`。Skill 主文件只做路由，
> `references/` 按任务渐进加载。

## 配置

配置优先级：CLI 参数 > 环境变量 > 配置文件 > 默认值。

默认配置文件是当前工作目录下的 `.codex-browser-bridge.toml`。也可以用 `CODEX_BRIDGE_CONFIG` 指定显式路径；设置后不会再回退读取工作目录配置：

```toml
profile = "basic"                # 推荐；basic | network | full
browser_family = "edge"          # chrome | edge | brave | opera | vivaldi
browser_instance = "..."         # extensionInstanceId；同 family 多实例时必填
upload_base = "C:/Users/me/Downloads"
max_text_bytes = 1048576
max_image_bytes = 3145728
```

MCP 客户端配置中也可以设置环境变量：

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp", "--profile", "network"],
      "transport": "stdio",
      "env": {
        "CODEX_BRIDGE_BROWSER_FAMILY": "edge",
        "CODEX_BRIDGE_BROWSER_INSTANCE": "extension-instance-id",
        "CODEX_BRIDGE_UPLOAD_BASE": "C:\\Users\\me\\Downloads",
        "CODEX_BRIDGE_MAX_TEXT_BYTES": "1048576",
        "CODEX_BRIDGE_MAX_IMAGE_BYTES": "3145728"
      }
    }
  }
}
```

`CODEX_BRIDGE_UPLOAD_BASE` 会启用 `codex_file_input`，并限制只能上传该目录下的文件。未显式设置时文件上传会被拒绝，因为不同 MCP 客户端启动 server 的工作目录不一定一致。

MCP 大响应有统一上限，避免 agent 意外收到多 MB 的 DOM、JavaScript、CDP 或截图 payload：

- `CODEX_BRIDGE_MAX_TEXT_BYTES` 限制每个 text content，默认 `1048576`。
- `CODEX_BRIDGE_MAX_IMAGE_BYTES` 限制每个 base64 image content，默认 `3145728`。
- 两者都有 8 MiB 硬上限。文本截断会带原始字节数标记；超大图片返回文本摘要，不返回无效的半截 base64。
- 也可以在 `.codex-browser-bridge.toml` 中使用 `max_text_bytes` / `max_image_bytes`，或用 CLI 参数 `--max-text-bytes` / `--max-image-bytes`。
- `CODEX_BRIDGE_SOCKET_DIR` 可以覆盖 Unix socket 发现目录；未设置时先尝试
  `$XDG_RUNTIME_DIR/codex-browser-use`，再回退到 `/tmp/codex-browser-use`。
- `CODEX_BRIDGE_NO_AUTO_FOREGROUND=1` 会关闭合成输入前的自动隐藏标签页激活；
  默认会激活标签页，关闭后可能恢复旧的静默输入丢失风险。

支持 MCP resources 的客户端可以读取 `codex://tabs`，它只返回当前 bridge session 拥有的标签页，不是所有 Chrome 标签页。支持 MCP prompts 的客户端还会看到 `login` 和 `extract-table` 两个提示模板。

## 全部 64 个 MCP 工具

### 浏览器选择 `[Browser]`
| 工具 | 说明 |
|------|------|
| `codex_list_browsers` | 列出实时实例的 `browser_id`、family、profile、版本、能力和 pipe 信息 |
| `codex_bind_browser` | 绑定精确 `browser_id`；仅单一匹配时允许按 family 绑定 |
| `codex_browser_history` | 按关键词、时间范围和 limit 读取浏览器历史 |
| `codex_browser_management` | 官方 windows/tabs/tabGroups/bookmarks 管理能力及变更审计；remove/delete/clear/close/discard 需要 `confirm=true` |
| `codex_browser_viewport` | 设置或重置官方浏览器 viewport 覆盖 |
| `codex_browser_visibility` | 在扩展声明该能力时查询或设置浏览器可见性 |

### 标签管理 `[Tabs]`
| 工具 | 说明 |
|------|------|
| `codex_list_tabs` | 列出当前 session 拥有的标签 |
| `codex_create_tab` | 创建空白标签 |
| `codex_close_tab` | 关闭标签 |
| `codex_user_tabs` | 列出浏览器所有标签（含未认领） |
| `codex_claim_tab` | 认领已有标签 |
| `codex_mark_tab` | 标记 deliverable 或跨 turn handoff |

### 导航 `[Navigation]`
| 工具 | 说明 |
|------|------|
| `codex_navigate` | 导航到 URL |
| `codex_reload` | 刷新页面 |
| `codex_navigate_back` | 后退 |
| `codex_navigate_forward` | 前进 |
| `codex_wait_for_load` | 等待 `document.readyState` 完成 |
| `codex_nav_and_wait` | 导航 + 等待（一次调用） |
| `codex_wait_for_element` | 轮询 CSS 选择器直到匹配（SPA 适用） |
| `codex_wait_for_url` | 轮询直到 URL 包含子串（SPA 路由） |

### DOM 与无障碍 `[DOM]`
| 工具 | 说明 |
|------|------|
| `codex_dom_snapshot` | 完整无障碍树；动作使用 `backendDOMNodeId`，不用 AX `nodeId`；大响应可能截断 |
| `codex_dom_get_visible` | 人类可读的可见 DOM 树 |
| `codex_dom_click` | 通过无障碍 nodeId 点击 |
| `codex_find_element` | 按 ARIA role + name 查找；`node_id` 为 backend DOM ID，只有 AX 节点的条目标记 `actionable=false` |
| `codex_click_element` | 点击 find_element 结果中的元素 |
| `codex_locator` | 严格 CSS/role/text/label/placeholder/test-id 定位与读写，支持双击；iframe/链式 locator 仍未完整对齐 |

### 页面检查 `[Page]`
| 工具 | 说明 |
|------|------|
| `codex_get_url` | 当前标签 URL |
| `codex_get_title` | 当前页面标题 |
| `codex_evaluate` | 执行无限制页面 JavaScript 并返回有上限的 JSON；这是 bridge 自有能力，不是官方只读 Playwright evaluate |
| `codex_page_assets` | 列出页面资源；可选抓取有已知大小且受限的内容 |
| `codex_console_logs` | 捕获一段时间内的 console 输出 |
| `codex_emulate_device` | 模拟移动端视口（`reset=true` 清除） |
| `codex_screenshot` | 截取视口截图，超大图片返回摘要 |
| `codex_screenshot_element` | 截取单个元素，超大图片返回摘要 |
| `codex_print_pdf` | 通过有界 CDP stream 渲染 PDF；只返回大小摘要，不嵌入 PDF 内容 |
| `codex_bring_to_front` | 激活后台标签（修复截图超时） |
| `codex_dialog` | 处理 alert / confirm / prompt |
| `codex_performance_metrics` | DOM 节点数、JS 堆、事件监听数（性能） |

### 输入交互 `[Input]`
| 工具 | 说明 |
|------|------|
| `codex_click` | 解析首个 CSS 匹配项并用真实 CDP 指针点击 |
| `codex_fill` | CSS 选择器填充输入框 |
| `codex_hover` | 解析 CSS 匹配项后用真实 CDP 指针悬停 |
| `codex_select_option` | 通过共享 locator 引擎选择确实存在的精确 option |
| `codex_drag` | 按 CUA 路径逐点拖拽；旧 from/to 自动转成两点路径 |
| `codex_cua_click` | CUA 坐标点击，支持 button `1..5`、修饰键和 `click_count=2` 双击 |
| `codex_cua_move` | CUA 指针移动，可携带修饰键 |
| `codex_cua_type` | 在当前焦点输入文字 |
| `codex_cua_keypress` | 按键序列（Enter、Ctrl+C 等） |
| `codex_cua_scroll` | CUA 坐标滚动，接受 `scrollX/scrollY` 和可选修饰键 |
| `codex_click_and_wait` | 点击 + 等待加载（一次调用） |
| `codex_form_fill` | 批量填表 `{selector: value}` |
| `codex_file_input` | 上传文件到 `<input type=file>` |

CUA 指针移动采用有上限的无 ack 窗口：当前扩展会执行 `mouseMoved`，但不返回响应；
因此 drag 延迟随 path 点数增长。

### 网络 `[Network]`
| 工具 | 说明 |
|------|------|
| `codex_network_cookies` | 读取 Cookie（默认脱敏） |
| `codex_network_set_cookie` | 设置 Cookie |
| `codex_delete_cookies` | 按名称删除 Cookie |
| `codex_storage` | 读写 localStorage |
| `codex_network_monitor` | 配对请求↔响应为结构化列表 |

### CDP 逃生口 `[CDP]`
| 工具 | 说明 |
|------|------|
| `codex_execute_cdp` | 执行明确 allowlist 允许的 CDP 诊断命令 |

### 会话 `[Session]`
| 工具 | 说明 |
|------|------|
| `codex_name_session` | 命名当前 session |
| `codex_finalize` | 发送官方 `turnEnded`，由扩展执行 cleanup，保留 handoff/deliverable 标记标签 |
| `codex_get_info` | 获取 bridge 运行时 + 扩展后端元数据 |
| `codex_doctor` | 自检诊断（pipe 连通性、延迟、版本） |

## CLI 用法

本版本还包括 `codex_clipboard`（session 虚拟剪贴板）、`codex_webmcp`（实验性的顶层文档注册/调用）和 `codex_content_export`（有界本地 GSuite/YouTube 导出）。

```bash
# MCP 模式（默认）
codex-browser-bridge --mode mcp

# 列出活跃管道
codex-browser-bridge --mode discover

# 交互式调试 REPL
codex-browser-bridge --mode cli

# 工具 profile
codex-browser-bridge --mode mcp --profile basic     # 45 个工具
codex-browser-bridge --mode mcp --profile network   # 63 个工具
codex-browser-bridge --mode mcp --profile full      # 全部 64 个（默认）

# 限制大 MCP 输出
codex-browser-bridge --mode mcp --max-text-bytes 1048576 --max-image-bytes 3145728
```

源码还提供复用同一 schema、profile、handler 和安全检查的 shell 命令。
`tools` 只列工具名，不连接浏览器；`tools NAME` 取单个 schema：

```bash
codex-browser-bridge tools codex_locator
codex-browser-bridge call codex_list_browsers '{}'
codex-browser-bridge --max-output-bytes 4096 batch <<'JSONL'
{"name":"codex_user_tabs","arguments":{}}
JSONL
```

每次 `call` 都是一个进程、一个浏览器 turn。进程退出时（包括失败）会结束该 turn，
因此不会留下属于已消失进程的未标记标签；`deliverable` 与 `handoff` 标记的标签会保留。
`batch` 从 stdin 每行读取一个 `{name, arguments}` JSON，共用一个 turn，遇错即停。
需要在多个独立命令之间共享同一 turn 时用 `--session-file <path>`；
`--session-file` 只适用于 `call` / `batch`，MCP stdio 和交互式 CLI 会拒绝；
`--keep-turn` 则显式保留 turn、不清理。Ctrl-C 与 SIGTERM 只在命令配置为 finalize
时尝试清理；`--keep-turn` 和未 finalize 的 `--session-file` 会保留 turn。
CLI 默认每条 JSON 结果上限 16384 bytes，图片只返回摘要、不输出 base64；
该上限独立于 MCP content 限额。CLI 是可选入口，不是独立的能力实现或 parity 验收标准。

## 架构

```
MCP Client (Claude Code / Cursor / OpenClaw)
        │ stdio JSON-RPC
        ▼
codex-browser-bridge (Rust 二进制)
        │ length-prefixed JSON-RPC frames
        ▼
本地传输（Windows named pipe / Linux Unix socket）
        │
        ▼
ChatGPT 桌面应用 → ChatGPT 扩展 → Chrome / Edge 标签页
```

## MCP 协议支持

本 bridge 是一个 **双纪元（dual-era）MCP 服务器**：

| 纪元 | 版本 | 行为 |
|------|------|------|
| Legacy | `2024-11-05` … `2025-11-25` | 经典 stdio 生命周期，`initialize` 版本协商（对旧客户端字节级兼容） |
| Modern | `2026-07-28` | 无状态请求：版本通过每个请求的 `_meta` 携带、`server/discover` 探测、结果携带 `resultType` + 缓存提示（`ttlMs`/`cacheScope`）、不支持的版本返回 `UnsupportedProtocolVersionError` (-32022) |

全部 64 个工具都携带行为注解（`readOnlyHint`、`destructiveHint`、`idempotentHint`、`openWorldHint`），供客户端审批 UX 使用。详见 [CHANGELOG.md](CHANGELOG.md)。

## 维护策略

这个项目刻意保持为很薄的兼容适配层，不是第二套浏览器运行时。ChatGPT 桌面应用或扩展升级本身不要求 bridge 发版；升级后按下面顺序冒烟验证：

1. `codex-browser-bridge --mode doctor`
2. `codex_list_browsers`
3. 多实例时 `codex_bind_browser`
4. `codex_get_info`
5. `codex_user_tabs`
6. 认领一个一次性标签页并读取标题或 DOM

只有 pipe 契约变化、需要更新兼容说明或修复真实回归时才发 patch 版本。不要把 Playwright 或另一套浏览器扩展作为兜底层。

## 安全

此工具让 agent 能访问你活跃的浏览器会话。

- 绝不暴露到网络端口
- 只为受信任的 MCP 客户端运行
- 在允许敏感操作前审查 agent 行为
- 避免在含密码、支付信息或管理后台的页面使用
- 分享截图/DOM/日志前脱敏
- `codex_file_input` 强制路径穿越防护（canonicalize + 前缀检查，10 MB 限制）
- 导航只接受 `http://` 和 `https://` URL
- Cookie 值默认脱敏；raw CDP 使用 allowlist，并阻止 browser/target/debugger/navigation/cookie/screenshot/PDF/file upload/event-producing enable/arbitrary Runtime JS/page-resource content/destructive storage 等敏感操作

## 开发

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge

cargo check --locked
cargo test --locked
cargo clippy --locked -- -D warnings
cargo build --locked --release
```

源码结构：

```
src/
  mcp/          MCP 服务（mod, lifecycle, types, schema, handlers, profiles）
  browser.rs    CDP + 浏览器操作
  client.rs     本地传输 + sticky attach
  security.rs   URL + 文件路径验证
  doctor.rs     本地传输诊断
  cli.rs        Shell 命令和交互式调试 REPL
  discovery.rs  本地传输自动发现
  protocol.rs   Length-prefixed JSON-RPC 帧
```

## 路线图

详见 [ROADMAP.md](ROADMAP.md)。亮点：

- 可选真实 E2E harness（ChatGPT 桌面应用 + Chrome/Edge）
- typed tool result schemas
- Linux Unix socket 实测与发布；macOS 仍需运行时验证

## 相关资源

- [examples/](examples/) — MCP 配置示例（Claude Code, Cursor, OpenClaw, Hermes Agent）
- [.claude-plugin/](.claude-plugin/plugin.json) — Claude Code 插件与 marketplace 元数据
- [skills/codex-browser/](skills/codex-browser/SKILL.md) — Agent skill（LLM 使用指南）
- [ROADMAP.md](ROADMAP.md) — 已发布版本与未完成项
- [CHANGELOG.md](CHANGELOG.md) — 发布历史
- [CONTRIBUTING.md](CONTRIBUTING.md) — 开发配置与规范
- [docs/release-process.md](docs/release-process.md) — tag、changelog、GitHub Release、npm 发布规则
- [scripts/live-e2e.ps1](scripts/live-e2e.ps1) — 可选真实 ChatGPT 桌面应用 + Chrome/Edge E2E 冒烟测试

## 许可证

MIT。独立于 OpenAI / Anthropic / Google 维护。

## 致谢

感谢 [LINUX DO](https://linux.do/) 社区的支持与反馈。
