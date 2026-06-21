<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    让 Claude Code 和其他 MCP Agent 通过 Codex Desktop 控制你现有的 Chrome 浏览器。
    <br>50 个 MCP 工具。纯 Rust。单文件二进制。零配置。
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

`codex-browser-bridge` 把你本机的 **Codex Desktop + Chrome** 变成一个任何 agent 都能控制的 MCP 服务器。

无需复制浏览器配置。无需 WebDriver。无需远程配置。它直接连接本机已存在的 Codex 浏览器 named pipe，使用相同的 JSON-RPC 协议，暴露 50 个 MCP 工具用于浏览器自动化。

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
- 执行原始 CDP 命令（Chrome DevTools Protocol 逃生口）
- 通过 `codex_doctor` 自检诊断

适用于需要真实浏览器会话的场景——后台管理系统、已登录的 Web 应用、本地开发服务器、文档网站。

## 快速安装

```bash
npm i -g @delicious233/codex-browser-bridge
```

或从 [GitHub Releases](https://github.com/DeliciousBuding/codex-browser-bridge/releases) 下载。

**需要：** Windows · Chrome · Codex Desktop · Codex Chrome Extension

## 30 秒接入 Claude Code

在 Claude Code MCP 设置中添加：

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp"],
      "transport": "stdio"
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

Cursor、OpenClaw、Hermes Agent 的配置见 [examples/](examples/)。

> 💡 **Agent skill 已内置。** 仓库 [`skills/codex-browser/SKILL.md`](skills/codex-browser/SKILL.md) 包含 LLM agent 使用全部 50 个工具的操作手册。将其 symlink 或复制到 agent 的 skills 目录即可（`~/.claude/skills/`、`~/.codex/skills/` 等）。

## 全部 50 个 MCP 工具

### 标签管理 `[Tabs]`
| 工具 | 说明 |
|------|------|
| `codex_list_tabs` | 列出当前 session 拥有的标签 |
| `codex_create_tab` | 创建空白标签 |
| `codex_close_tab` | 关闭标签 |
| `codex_user_tabs` | 列出浏览器所有标签（含未认领） |
| `codex_claim_tab` | 认领已有标签 |

### 导航 `[Navigation]`
| 工具 | 说明 |
|------|------|
| `codex_navigate` | 导航到 URL |
| `codex_reload` | 刷新页面 |
| `codex_navigate_back` | 后退 |
| `codex_navigate_forward` | 前进 |
| `codex_wait_for_load` | 等待 `document.readyState` 完成 |
| `codex_nav_and_wait` | 导航 + 等待（一次调用） |

### DOM 与无障碍 `[DOM]`
| 工具 | 说明 |
|------|------|
| `codex_dom_snapshot` | 完整无障碍树（含 nodeId） |
| `codex_dom_get_visible` | 人类可读的可见 DOM 树 |
| `codex_dom_click` | 通过无障碍 nodeId 点击 |
| `codex_find_element` | 按 ARIA role + name 查找元素 |
| `codex_click_element` | 点击 find_element 结果中的元素 |

### 页面检查 `[Page]`
| 工具 | 说明 |
|------|------|
| `codex_screenshot` | 截取视口 PNG 截图 |
| `codex_bring_to_front` | 激活后台标签（修复截图超时） |
| `codex_evaluate` | 执行 JavaScript，返回 JSON 结果 |
| `codex_page_assets` | 列出页面资源（图片/CSS/JS/字体） |
| `codex_dialog` | 处理 alert / confirm / prompt |

### 输入交互 `[Input]`
| 工具 | 说明 |
|------|------|
| `codex_click` | CSS 选择器点击（JS click） |
| `codex_fill` | CSS 选择器填充输入框 |
| `codex_cua_click` | 精确坐标点击（CDP 鼠标事件） |
| `codex_cua_type` | 在当前焦点输入文字 |
| `codex_cua_keypress` | 按键序列（Enter、Ctrl+C 等） |
| `codex_cua_scroll` | 坐标处滚动 |
| `codex_click_and_wait` | 点击 + 等待加载（一次调用） |
| `codex_form_fill` | 批量填表 `{selector: value}` |
| `codex_file_input` | 上传文件到 `<input type=file>` |

### 网络 `[Network]`
| 工具 | 说明 |
|------|------|
| `codex_network_cookies` | 读取 Cookie（默认脱敏） |
| `codex_network_set_cookie` | 设置 Cookie |

### CDP 逃生口 `[CDP]`
| 工具 | 说明 |
|------|------|
| `codex_execute_cdp` | 执行任意 CDP 命令（allowlist 保护） |

### 会话 `[Session]`
| 工具 | 说明 |
|------|------|
| `codex_name_session` | 命名当前 session |
| `codex_finalize` | 结束 session，清理标签 |
| `codex_get_info` | 获取扩展后端元数据 |
| `codex_doctor` | 自检诊断（pipe 连通性、延迟、版本） |

## CLI 用法

```bash
# MCP 模式（默认）
codex-browser-bridge --mode mcp

# 列出活跃管道
codex-browser-bridge --mode discover

# 交互式调试 REPL
codex-browser-bridge --mode cli

# 工具 profile
codex-browser-bridge --mode mcp --profile basic     # 32 个工具
codex-browser-bridge --mode mcp --profile network   # 48 个工具
codex-browser-bridge --mode mcp --profile full      # 全部 50 个（默认）
```

## 架构

```
MCP Client (Claude Code / Cursor / OpenClaw)
        │ stdio JSON-RPC
        ▼
codex-browser-bridge (Rust 二进制)
        │ length-prefixed JSON-RPC frames
        ▼
Windows Named Pipe \\.\pipe\codex-browser-use-*
        │
        ▼
Codex Desktop → Chrome Extension → Chrome 标签页
```

## 安全

此工具让 agent 能访问你活跃的浏览器会话。

- 绝不暴露到网络端口
- 只为受信任的 MCP 客户端运行
- 在允许敏感操作前审查 agent 行为
- 避免在含密码、支付信息或管理后台的页面使用
- 分享截图/DOM/日志前脱敏
- `codex_file_input` 强制路径穿越防护（canonicalize + 前缀检查，10 MB 限制）
- Cookie 值默认脱敏；CDP allowlist 阻止危险域

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
  mcp/          MCP 服务（mod, types, schema, handlers, profiles）
  browser.rs    CDP + 浏览器操作
  client.rs     Named pipe 传输 + sticky attach
  security.rs   URL + 文件路径验证
  doctor.rs     Pipe 诊断
  cli.rs        交互式调试 REPL
  discovery.rs  Pipe 自动发现
  protocol.rs   Length-prefixed JSON-RPC 帧
```

## 路线图

详见 [ROADMAP.md](ROADMAP.md)。亮点：

- `codex_network_monitor` — 请求/响应检查
- `codex_emulate_device` — 移动端视口模拟
- `codex_storage` — localStorage / sessionStorage 访问
- v2.0.0: 跨平台（macOS / Linux via Unix domain socket）

## 相关资源

- [examples/](examples/) — MCP 配置示例（Claude Code, Cursor, OpenClaw, Hermes Agent）
- [skills/codex-browser/](skills/codex-browser/SKILL.md) — Agent skill（LLM 使用指南）
- [ROADMAP.md](ROADMAP.md) — 完整路线图（含 SUPER 评分）
- [CHANGELOG.md](CHANGELOG.md) — 发布历史
- [CONTRIBUTING.md](CONTRIBUTING.md) — 开发配置与规范

## 许可证

MIT。独立于 Codex / Anthropic / Google 维护。

## 致谢

感谢 [LINUX DO](https://linux.do/) 社区的支持与反馈。
