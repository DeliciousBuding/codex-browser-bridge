# codex-browser-bridge

用于在 Claude Code、Antigravity、Cursor、OpenClaw、Hermes Agent 和 shell 脚本中复用本机已安装的 Codex **Browser Use**（`node_repl`）与 **Computer Use**（`cua_repl`）运行时的 MCP 服务与 CLI。

[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/actions)
[![Coverage](https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://codecov.io/gh/DeliciousBuding/codex-browser-bridge)

[项目网站](https://deliciousbuding.github.io/codex-browser-bridge/) · [English](README.md) · [原生接入指南](docs/native-computer-use.md) · [升级到 3.0.1](docs/upgrade-v3.md)

## 选择入口

| 入口 | 运行时 | 用途 |
|---|---|---|
| `--surface native-browser`（默认） | Codex `node_repl` | 复用已安装的 Codex Browser/Chrome 插件 JavaScript API（`agent.browsers`、`tab.playwright`、`tab.dom_cua`）。 |
| `--surface computer` | Codex `cua_repl` | 复用已安装的 Computer Use `cua` API，支持桌面窗口与统一浏览器目标。 |

不带 `--surface` 的裸命令默认使用 `native-browser`。[原生接入指南](docs/native-computer-use.md)说明如何发现运行时、环境变量策略和授权流程。Browser Use 与 Computer Use 分别注册，各自拥有独立的 worker 状态。

## 快速开始

### 插件与 Skill 安装

```bash
# Claude Code 插件
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge

# Google Antigravity（agy CLI，直接导入 .claude-plugin/plugin.json）
agy plugin install https://github.com/DeliciousBuding/codex-browser-bridge

# Agent Skills（skills.sh）
npx skills add DeliciousBuding/codex-browser-bridge
```

仓库同时提供 Cursor 原生插件清单（[.cursor-plugin/plugin.json](.cursor-plugin/plugin.json)）与 Official MCP Registry 清单（[server.json](server.json)），并在 [examples/](examples/) 中提供 VS Code / OpenCode / Antigravity / Cursor 等客户端模板。插件注册两个原生 MCP 服务（`codex-browser` 与 `codex-computer`）；请先安装并配置 Codex 原生运行时。如果本机只安装了其中一个运行时，另一个服务启动失败属于预期行为；如需单入口，可使用手工配置只保留对应项。

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

用 `batch` 在多条原生 JS 调用间复用同一个 worker。`tools`、`call`、`batch` 不能与 `--mode` 同时使用；`--image-dir` 只对 `call` 与 `batch` 有效。与 `--image-dir` 一起使用 `--omit-inline-images` 可保留已保存的文件与 `bridge_artifacts`，同时从 stdout 中移除 base64 `data`。

**需要：** 对应入口的原生运行时已安装和配置；仅 npm/npx 路径需要 Node.js 18+。

## 范围

二进制是一个约 1,700 行的薄无状态 Rust stdio 中继：发现所选 launch row、启动一个自有 worker、双向转发 MCP 帧、应用该行声明的工具过滤与环境策略，并在退出时收掉 worker 进程树。

它不实现 CDP、DOM 快照、定位器、观察游标、工具 profile 或浏览器 session；这些属于已安装运行时，客户端看到的行为、权限和目录都以运行时为准。传输路径只有一条：stdio 直连该运行时。

## 升级说明

- **从 2.x 或 3.0.0 升级到 3.0.1：** 阅读 [docs/upgrade-v3.md](docs/upgrade-v3.md)（兼容浏览器适配层及其结构化工具已移除）。
- **从 1.x 升级到 2.x：** 阅读 [docs/upgrade-v2.md](docs/upgrade-v2.md)。

## 平台支持

| 平台 | 状态 |
|------|------|
| Windows x64 / arm64 | 已发布二进制 |
| Linux GNU x64 / arm64 | 已发布二进制；glibc >= 2.39 |
| macOS arm64 / x64 | 已发布二进制；需在 macOS 提示时为已安装运行时授予「辅助功能」与「屏幕录制」权限（TCC） |

中继行为随对应平台已安装运行时而定。Linux/WSL 进程需要本环境内的运行时；桥接器不会跨 OS 边界转接 Windows worker。

## MCP 行为

- 中继原样转发客户端握手、工具目录、分页 cursor、未知工具字段、图片与错误，不重写 schema 或原生扩展；版本协商与客户端请求超时由外部宿主负责。
- `tools/call` 缺少 Codex 关联元数据时，自动补齐 `_meta.callId`、`_meta["x-codex-turn-metadata"]`（`session_id` 与 `turn_id`）、`_meta.sessionId` 与 `_meta.threadId`。
- `enabled_tools` 先于 `disabled_tools` 生效，覆盖对外工具目录、直接 `tools/call` 请求和会话结束时的 `turn_ended` 钩子。
- 应用授权使用双向 MCP form elicitation（`elicitation/create`），`accept`、`decline`、`cancel` 原样透传；CLI 非交互，返回标准 MCP 结果 `{"action": "cancel"}`。
- 在 EOF、`SIGINT` 或 `SIGTERM` 时取消进行中的任务、发送 `turn_ended`、关闭 worker stdin，并收掉自有进程树（Windows Job Object、Unix 进程组）。超时或中断的调用绝不重放。

## 维护

上游 Codex 桌面应用升级本身不要求发版。升级后先验证中继：

1. `codex-browser-bridge --surface native-browser --mode doctor`
2. `codex-browser-bridge --surface computer --mode doctor`

只有启动器/传输契约变化，或修复本仓库真实回归时才发 patch。

## 安全

这个工具在你的浏览器和桌面会话内操作。

- 不暴露到网络端口，只为受信任的 MCP 客户端运行。
- 已安装运行时的权限与授权弹窗是权威来源；桥接器不叠加第二套权限层，也不重新实现 CDP 访问控制。
- 敏感操作前审查 agent 行为；避免密码、支付和管理后台页面。
- 分享截图、DOM 或日志前脱敏。
- 无遥测，也没有隐藏的回退运行时。

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
- [docs/native-computer-use.md](docs/native-computer-use.md) — 运行时发现、环境策略、授权与验证状态
- [ROADMAP.md](ROADMAP.md) · [CHANGELOG.md](CHANGELOG.md)

## 许可证

MIT。独立于 OpenAI、Anthropic 和 Google 维护。

感谢 [LINUX DO](https://linux.do/) 社区的支持与反馈。
