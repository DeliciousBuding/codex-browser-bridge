# codex-browser-bridge

用于在 Claude Code、Antigravity、Cursor、OpenClaw、Hermes Agent 和 shell 脚本中复用本机已安装的 Codex **Browser Use**（`node_repl`）与 **Computer Use**（`cua_repl`）运行时的 MCP 服务与 CLI；同时保留直连 Chromium 扩展的兼容入口（`--surface browser`）。

[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/actions)
[![Coverage](https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://codecov.io/gh/DeliciousBuding/codex-browser-bridge)

[项目网站](https://deliciousbuding.github.io/codex-browser-bridge/) · [English](README.md) · [原生接入指南](docs/native-computer-use.md) · [升级到 3.0](docs/upgrade-v3.md)

## 选择入口

| 入口 | 运行时 | 用途 |
|---|---|---|
| `--surface native-browser` | Codex `node_repl` | 复用已安装的 Codex Browser/Chrome 插件 JavaScript API（`agent.browsers`、`tab.playwright`、`tab.dom_cua`）。 |
| `--surface computer` | Codex `cua_repl` | 复用已安装的 Computer Use `cua` API，支持桌面窗口与统一浏览器目标。 |
| `--surface browser` | Chromium 扩展管道 | 兼容模式：面向已有 2.x 工作流或未安装原生 Codex 运行时主机的结构化 `codex_*` 工具。 |

[原生接入指南](docs/native-computer-use.md)说明如何发现运行时、环境变量策略和授权流程。Browser Use 与 Computer Use 分别注册，各自拥有独立的 worker 状态。

## 快速开始

### Claude Code 插件

```bash
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

插件注册两个原生 MCP 服务（`codex-browser` 与 `codex-computer`）。请先安装并配置 Codex 原生运行时；插件不打包运行时。如果本机只安装了其中一个运行时，另一个服务启动失败属于预期行为；如需单入口，可使用手工配置只保留对应项。

### 手工 MCP 配置

```bash
npm i -g @delicious233/codex-browser-bridge   # 或下载 release 二进制
```

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "native-browser", "--mode", "mcp"]
    },
    "codex-computer": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "computer", "--mode", "mcp"]
    }
  }
}
```

只安装了一个运行时，或只需要一套工具时，可以只保留对应注册。客户端模板见 [examples/](examples/)。

### CLI

```bash
codex-browser-bridge --surface native-browser --mode doctor
codex-browser-bridge --surface computer --mode doctor
codex-browser-bridge --surface computer call js '{"code":"await cua.listWindows();"}'
```

用 `batch` 在多条原生 JS 调用间复用同一个 worker。已有 `codex_*` 脚本请显式使用 `--surface browser`；不带 `--surface` 的裸命令仍默认 `--surface browser` 以保持脚本向后兼容。

**需要：** 对应入口的原生运行时已安装和配置；仅 npm/npx 路径需要 Node.js 18+。

## 升级说明

- **从 2.x 升级到 3.0：** 阅读 [docs/upgrade-v3.md](docs/upgrade-v3.md)（插件与客户端模板已切换为原生 `node_repl` + `cua_repl`）。
- **从 1.x 升级到 2.x：** 阅读 [docs/upgrade-v2.md](docs/upgrade-v2.md)。

## 兼容性

最近一次扩展实测审计：2026-10-24，ChatGPT 桌面应用（`OpenAI.Codex` 包）`26.1016.8120.0`，Edge 扩展 `1.26.1020.11845`（源码/协议对照基线固定于 `26.1111.21308`）。原生 `node_repl` 与 `cua_repl` 的验证状态见 [docs/native-computer-use.md](docs/native-computer-use.md)。

| 层面 | 状态 |
|------|------|
| ChatGPT 桌面应用 + 浏览器扩展 | 当前频道，已实测 |
| Chrome / Edge | 支持；本次审计实测 Edge |
| Brave / Opera / Vivaldi | 预计共用 Chromium/CDP 管线，未实测 |
| MCP `2024-11-05` … `2025-11-25` | 支持 Legacy 生命周期 |
| MCP `2026-07-28` | 支持无状态生命周期与 discovery |
| Windows x64 / arm64 | 已发布二进制；x64 通过合成 Edge parity 冒烟 |
| Linux GNU x64 / arm64 | 已发布二进制；原生门禁与合成扩展冒烟通过，不等于完整能力面 |
| macOS | 源码编译与 CI 测试通过；无预编译二进制 |

一个 MCP session 同时只控制一个扩展实例，不同浏览器 profile 的标签页不会合并。Linux 依次发现 `CODEX_BRIDGE_SOCKET_DIR`、`$XDG_RUNTIME_DIR/codex-browser-use`、`/tmp/codex-browser-use`，并校验 socket 所有权、模式和 peer UID。Linux/WSL 进程需要本环境内的 socket，不会自动跨 OS 发现 Windows pipe。

## 兼容扩展适配层（`--surface browser`）

以下各节**仅适用于**显式指定 `--surface browser`（直连 Chromium 扩展管道）的兼容模式。原生 `node_repl` 与 `cua_repl` 入口会忽略 `.codex-browser-bridge.toml` 与 `CODEX_BRIDGE_*` 浏览器 profile 配置。

### 浏览器实例选择

- `codex_list_browsers` 列出所有实时实例；`codex_bind_browser` 绑定精确 `browser_id`（`metadata.extensionInstanceId`）。pipe 名只是传输信息，不是身份。
- 自动或按 family 选择要求 inventory 完整，且只有一个匹配实例；探测超时和未识别端点一律 fail closed。
- 重连始终固定在所选实例。会话仍拥有标签时，重新绑定必须先 `codex_finalize`，或传 `end_previous_turn: true`。
- 进程级固定用 `--browser-family` / `--browser-instance`、对应的 `CODEX_BRIDGE_*` 环境变量或 TOML 键。`--pipe` 是底层覆盖，不能与选择器同时使用。

### 配置（仅 `--surface browser`）

优先级：CLI 参数 > 环境变量 > 配置文件 > 内置默认值。默认读取工作目录下的 `.codex-browser-bridge.toml`；设置 `CODEX_BRIDGE_CONFIG` 后只读该路径，不再回退。

```toml
profile = "basic"                # basic | network | full
browser_family = "edge"          # chrome | edge | brave | opera | vivaldi
browser_instance = "..."         # extensionInstanceId；同 family 多实例时必填
upload_base = "C:/Users/me/Downloads"
max_text_bytes = 1048576
max_image_bytes = 3145728
```

- 只有默认文件不存在时才可省略。文件不可读、TOML 损坏、未知键、空 selector、非法 profile 和超出 `1024..=8388608` 的上限都会在浏览器操作前报错。`--mode doctor` 返回有效 selector 及来源。
- `CODEX_BRIDGE_UPLOAD_BASE` 启用 `codex_file_input`，上传范围限制在该目录（canonicalize + 前缀检查，10 MB 上限）。未设置时上传保持禁用。
- `CODEX_BRIDGE_MAX_TEXT_BYTES` / `CODEX_BRIDGE_MAX_IMAGE_BYTES` 限制每个 MCP content 项。文本截断会标注原始字节数；超大图片返回文本摘要。
- `CODEX_BRIDGE_NETWORK_POLICY`（或 `--network-policy`）为每次 attach 携带 Browser Use 网络策略。内联 JSON：`{"timeout_ms":1..30000,"constraints":[{"kind":"domains"|"origins","mode":"allowlist"|"denylist","patterns":[...]}]}`。省略即全放行，且只影响之后的 attach。
- attach 后启用焦点模拟，输入和截图不激活标签页、不抢焦点；需要展示页面时显式调用 `codex_bring_to_front`。滚动沿用 `Input.synthesizeScrollGesture`，超时代表结果未知，再次滚动前先检查页面。
- `codex://tabs` 作为 MCP resource 暴露本 session 拥有的标签页；`login` 和 `extract-table` 覆盖两个常见工作流。

### 兼容 MCP 工具（`--surface browser`）

Profile：`basic` 46 个工具、`network` 65 个工具、`full` 66 个工具。所有工具携带 MCP 行为注解（`readOnlyHint`、`destructiveHint`、`idempotentHint`、`openWorldHint`）。

<details>
<summary>展开全部 66 个兼容扩展工具</summary>

#### 浏览器选择 `[Browser]`

| 工具 | 说明 |
|------|------|
| `codex_list_browsers` | 列出实时实例的 `browser_id`、family、profile、版本、能力与传输信息 |
| `codex_bind_browser` | 绑定精确 `browser_id`；仅 family 绑定要求只有一个匹配实例 |
| `codex_browser_history` | 按关键词、时间范围和 limit 读取历史 |
| `codex_browser_management` | 官方 windows/tabs/tabGroups/bookmarks 管理与审计；破坏性调用需要 `confirm=true` |
| `codex_browser_viewport` | 设置或重置官方 viewport 覆盖 |
| `codex_browser_visibility` | 扩展声明该能力时查询或设置浏览器可见性 |

### 标签页 `[Tabs]`

`codex_observe` 在操作前启动有界 tab/document 观察，返回 frame、dialog ID、事件/日志游标和明确 gap。`codex_file_chooser` 捕获 chooser ID 与 `multiple`；上传仍需配置根目录和 `colocated_browser=true`。

| 工具 | 说明 |
|------|------|
| `codex_list_tabs` | 列出本 session 拥有的标签页；`selected_only=true` 解析当前活动的已拥有标签页（`Tabs.selected`） |
| `codex_create_tab` | 创建空白标签页 |
| `codex_close_tab` | 按 ID 关闭标签页 |
| `codex_user_tabs` | 列出浏览器全部标签页，含未认领项 |
| `codex_claim_tab` | 用最新枚举的标题/URL 认领用户标签页 |
| `codex_mark_tab` | 标记 deliverable 或跨 turn handoff |
| `codex_observe` | 启动有界观察；`wait_until=networkidle`，`wait_navigation` 支持 `url_glob`/`url_regex`，`levels` 过滤日志 |
| `codex_file_chooser` | 捕获 document-bound 文件选择器 |

### 导航 `[Navigation]`

| 工具 | 说明 |
|------|------|
| `codex_navigate` | 导航到 URL |
| `codex_reload` | 刷新页面 |
| `codex_navigate_back` | 后退 |
| `codex_navigate_forward` | 前进 |
| `codex_wait_for_load` | 等待 `document.readyState` 完成 |
| `codex_nav_and_wait` | 导航并等待，一次调用 |
| `codex_wait_for_element` | 轮询 CSS 选择器直到匹配 |
| `codex_wait_for_url` | 轮询直到精确 `url`、子串 `pattern`、`url_glob` 或 `url_regex` 匹配 |

### DOM 与无障碍 `[DOM]`

| 工具 | 说明 |
|------|------|
| `codex_dom_snapshot` | 无障碍树；`include_iframes=true` 附加有界的同源 frame 文本；动作用 `backendDOMNodeId` |
| `codex_dom_get_visible` | 人类可读的可见 DOM 树 |
| `codex_dom_click` | 按无障碍节点 ID 点击 |
| `codex_find_element` | 按 ARIA role 与 name 查找；仅 AX 节点标记 `actionable=false` |
| `codex_click_element` | 点击 `codex_find_element` 返回的元素 |
| `codex_locator` | 严格 CSS/role/text/label/test-id 读写与操作，支持 `and`/`or`/`has`/`has_not`、`element_info`、双击、`force` 与 `trial`；仅同源 frame |

### 页面检查 `[Page]`

| 工具 | 说明 |
|------|------|
| `codex_get_url` | 当前标签页 URL |
| `codex_get_title` | 当前页面标题 |
| `codex_evaluate` | 执行页面 JavaScript 并返回有界 JSON；bridge 自有能力，不是 Playwright 只读 evaluate |
| `codex_page_assets` | 列出页面资源，可选抓取有界缓存内容 |
| `codex_console_logs` | 捕获窗口 console 输出 |
| `codex_emulate_device` | 模拟移动端视口（`reset=true` 清除） |
| `codex_screenshot` | 截取视口；超大图片返回摘要 |
| `codex_screenshot_element` | 截取单个元素，支持 `padding`（0..256 CSS px）和 `include_info=true` 返回 `elementInfo` 元数据 |
| `codex_print_pdf` | 通过有界 CDP stream 渲染；只返回大小摘要，不返回 PDF 字节 |
| `codex_bring_to_front` | 显式激活标签页供查看 |
| `codex_dialog` | 处理 alert / confirm / prompt |
| `codex_performance_metrics` | DOM 节点数、JS 堆和事件监听数 |

### 输入交互 `[Input]`

CUA 指针移动须在有界窗口内收到回执，且从不重放。drag 耗时随路径点数增长。

| 工具 | 说明 |
|------|------|
| `codex_click` | 解析首个 CSS 匹配并用可信 CDP 指针点击 |
| `codex_fill` | 按 CSS 选择器填充输入框 |
| `codex_hover` | 用可信 CDP 指针悬停首个 CSS 匹配 |
| `codex_select_option` | 通过共享 locator 引擎选择已存在的精确 option |
| `codex_drag` | 按 CUA 路径逐点拖拽；旧 `from`/`to` 映射为两点 |
| `codex_cua_click` | CUA 坐标点击，支持 button `1..5`、修饰键和 `click_count=2` |
| `codex_cua_move` | CUA 指针移动，可带修饰键 |
| `codex_cua_type` | 在当前焦点输入文本 |
| `codex_cua_keypress` | 按键序列，如 Enter、Ctrl+C |
| `codex_cua_scroll` | CUA 滚动，接受 `scrollX`/`scrollY` 和可选修饰键 |
| `codex_click_and_wait` | 点击并等待加载，一次调用 |
| `codex_form_fill` | 用 `{selector: value}` 映射批量填表 |
| `codex_file_input` | 上传文件到 `<input type=file>` |

### 网络 `[Network]`

| 工具 | 说明 |
|------|------|
| `codex_network_cookies` | 读取 Cookie，值默认脱敏 |
| `codex_network_set_cookie` | 设置 Cookie |
| `codex_delete_cookies` | 按名称删除 Cookie |
| `codex_storage` | 读写 localStorage |
| `codex_network_monitor` | 将请求与响应配对为结构化列表 |

### CDP 逃生口 `[CDP]`

| 工具 | 说明 |
|------|------|
| `codex_execute_cdp` | 执行 allowlist 内的 CDP 诊断 |

### 会话 `[Session]`

| 工具 | 说明 |
|------|------|
| `codex_name_session` | 命名当前 session |
| `codex_finalize` | 发送官方 `turnEnded`；cleanup 保留已标记的 handoff/deliverable 标签页 |
| `codex_get_info` | 读取 bridge 运行时与扩展后端元数据 |
| `codex_doctor` | 传输健康、延迟和版本自检 |

### Bridge 状态与产物

| 工具 | 说明 |
|------|------|
| `codex_clipboard` | 有界 session 虚拟剪贴板与粘贴集成 |
| `codex_webmcp` | 实验性顶层文档注册列表与调用 |
| `codex_content_export` | 有界本地 GSuite/YouTube 导出，路径作用域为 `bridge_host` |

</details>

### 兼容 CLI（`--surface browser`）

```bash
codex-browser-bridge --surface browser --mode mcp          # stdio MCP server
codex-browser-bridge --surface browser --mode discover     # 列出实时扩展管道
codex-browser-bridge --surface browser --mode cli          # 旧版交互式 REPL
codex-browser-bridge --surface browser tools codex_locator # 单个 schema，不连接
codex-browser-bridge --surface browser --mode doctor       # 管道诊断
```

`call` 和 `batch` 复用与 MCP 相同的 schema、profile、handler 和安全检查。`batch` 从 stdin 每行读一个 `{name, arguments}`，遇错即停。`--session-file <path>` 让多个独立命令共用一个 turn：v2 文件绑定精确扩展实例，每次调用持 OS 锁，旧版或损坏文件直接失败而不会重置。`--keep-turn` 显式保留未结束的 turn。CLI 的 JSON 结果默认 16384 bytes，图片只返回摘要；该预算与 MCP content 限额相互独立。

## 架构

共享原生 stdio 中继与独立兼容扩展通道见[架构说明](ARCHITECTURE.md)。

## MCP 协议支持（`--surface browser`）

- **Legacy**（`2024-11-05` … `2025-11-25`）：经典 stdio 生命周期，`initialize` 版本协商，对旧客户端字节级兼容。
- **Modern**（`2026-07-28`）：无状态请求，版本放在 `_meta`，支持 `server/discover`、`resultType` 和 `ttlMs`/`cacheScope`。不支持的版本返回 `-32022`。
- `codex://tabs`、`login` 和 `extract-table` 提供给支持 MCP resources/prompts 的客户端。
- 原生中继（`--surface native-browser` 与 `--surface computer`）直接透传 MCP 帧，不重写宿主握手或协议扩展。

## 维护

上游 Codex 桌面应用或浏览器扩展升级本身不要求发版。升级后优先验证原生中继：

1. `codex-browser-bridge --surface native-browser --mode doctor`
2. `codex-browser-bridge --surface computer --mode doctor`

如果同时使用 `--surface browser` 兼容模式，可运行 `codex-browser-bridge --surface browser --mode doctor`，检查 `codex_list_browsers` / `codex_user_tabs` 并认领一个一次性标签页读取标题或 DOM。

只有传输/启动器契约变化或修复真实回归时才发 patch。兼容能力矩阵在 [docs/official-parity.md](docs/official-parity.md)，漂移基线在 [docs/official-surface-baseline.json](docs/official-surface-baseline.json)。

## 安全

这个工具在你的浏览器和桌面会话内操作。

- 不暴露到网络端口，只为受信任的 MCP 客户端运行。
- 敏感操作前审查 agent 行为；避免密码、支付和管理后台页面。
- 分享截图、DOM 或日志前脱敏。
- `codex_file_input` 会 canonicalize 路径、限制在配置根目录内，并限制 10 MB。
- 导航只接受 `http://` 和 `https://`。Cookie 值默认脱敏。raw CDP 仅限 allowlist，并阻止 browser、target、debugger、navigation、cookie、screenshot、PDF、upload、event-enable、任意 Runtime JS、page-resource content 和破坏性 storage 操作。

## 开发

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
cargo check --locked
cargo test --locked
cargo clippy --locked -- -D warnings
cargo build --locked --release
```

模块图与设计边界见 [ARCHITECTURE.md](ARCHITECTURE.md)，开发规范见 [CONTRIBUTING.md](CONTRIBUTING.md)，发布流程见 [docs/release-process.md](docs/release-process.md)。

## 相关资源

- [examples/](examples/) — Claude Code、Antigravity、Cursor、OpenClaw、Hermes Agent 和原生 Computer Use 的 MCP 配置
- [skills/codex-browser/](skills/codex-browser/SKILL.md) — 渐进加载的 agent skill
- [ARCHITECTURE.md](ARCHITECTURE.md) — 模块图与设计决策
- [ROADMAP.md](ROADMAP.md) · [CHANGELOG.md](CHANGELOG.md)
- [docs/official-parity.md](docs/official-parity.md) — 能力矩阵与漂移基线

## 许可证

MIT。独立于 OpenAI、Anthropic 和 Google 维护。

感谢 [LINUX DO](https://linux.do/) 社区的支持与反馈。
