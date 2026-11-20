# AGENTS.md

禁止提交 `.env`、凭据、私钥。提交前 `git diff --staged` 自查。

## 仓库级 Skill

本仓库包含项目级 skill bundle：`skills/codex-browser/SKILL.md` + `references/`

`SKILL.md` 只负责路由和工作流，任务细节按需读取一个 reference。支持项目级 skill 的客户端会自动发现；Claude Code 可通过仓库内 marketplace 把 skill 与 MCP server 作为同一个插件安装。

## 插件分发层

插件是分发层，MCP server 是执行层，两者不重复实现：

- `.claude-plugin/plugin.json` + `.claude-plugin/marketplace.json`：Claude Code 插件，复用根 `mcp.json`
- `mcp.json` 通过 `npx` 启动固定版本的 npm 包；插件本身不分发 Windows 二进制。全局 npm 安装是可选的快速路径，手工 MCP 示例仍使用它。

Codex / ChatGPT desktop 已经能通过内置原生运行时直接控制浏览器与桌面，不再把 bridge 包装成 Codex 插件，也不安装这个重复入口。bridge 作为通用 MCP/CLI 桥接层，供 Claude Code、Cursor、Antigravity、OpenClaw、Hermes Agent 等非 Codex 客户端复用已安装的原生运行时。

`v3.0+` 只有两个入口：`codex-browser`（`--surface native-browser`，默认值，转发本机已安装的 `node_repl`）与 `codex-computer`（`--surface computer`，转发 `cua_repl`）。旧的 Chromium 扩展适配层、`codex_*` 结构化工具、profile 与会话文件已整体删除，不要重新引入。

## 中继实现规范

本项目是薄无状态 stdio 中继，不是浏览器自动化实现。改动应守住以下边界：

- 工具目录、schema、分页 cursor、图片、错误、`elicitation/create` 审批和原生扩展字段一律原样透传，不做归一化或重编码。
- 不在 bridge 内实现 CDP、DOM/无障碍快照、定位器、观察游标、工具 profile、浏览器 session 或权限 allowlist；这些属于已安装运行时。
- 工具过滤以 launch row 的 `enabled_tools` / `disabled_tools` 为准，`enabled_tools` 先于 `disabled_tools`，并同时覆盖目录、直接 `tools/call` 与 `turn_ended` 清理钩子。
- 补齐调用关联元数据（`_meta.callId`、`_meta["x-codex-turn-metadata"]`、`_meta.sessionId`、`_meta.threadId`）时保持幂等；畸形元数据返回 `-32602`，不要关闭会话。
- 子进程环境遵循 Codex CLI 策略：`env_clear()` 后只保留平台核心变量、launch row 的 `env` / `env_vars`、自定义 CA 变量；剥离 Codex 内部认证 token；Windows 裸命令按构造后的 `PATH` + `PATHEXT` 解析。
- 每个连接或 CLI 调用拥有一个 worker 进程树；EOF/中断时取消在途请求、发 `turn_ended`、关闭 stdin、收掉进程树，超时或中断的调用绝不重放。
- 帧与缓冲区上限保持固定：单帧 64 MiB、每侧出站积压 8 MiB / 256 帧、`batch` 单行 1 MiB、目录遍历 100 页 / 2048 项 / 64 KiB cursor。
- 重启 worker、回退到别的运行时、或静默重定向到另一 surface 都属于错误行为。

## 测试

- 单元与集成测试使用 fixture launch row 和 mock worker，不依赖真实桌面应用。
- `tests/native_process.rs` 覆盖自有进程树的创建与收尾。
- 等待必须 bounded：正向往返与 spawned task join 用明确 timeout，负向断言（不得转发某请求）用短 grace window。
- 测试只验证现役中继行为，不锁文档字样或静态措辞。

## 构建

```bash
cargo check --locked              # 快速检查
cargo test --locked                # 全量测试
cargo clippy --locked --all-targets -- -D warnings  # lint
cargo build --locked --release     # 发布构建 → target/release/codex-browser-bridge.exe
npm --prefix npm test              # npm installer/package helper 测试
```

Release 时遵循 `docs/release-process.md` 的版本、changelog、资产和 npm 合同；那里维护完整文件清单与发布顺序，本文件不复刻。

发布前运行 `node scripts/check-plugin-surface.js` 与 `claude plugin validate --strict .`。前者在 CI 中验证 Claude manifest、唯一 MCP 配置、marketplace 和示例 npm pins 不漂移，并阻止 Codex 插件分发文件重新进入仓库。

`mcp.json` 的 npm 包版本必须与 `.claude-plugin/plugin.json`、`npm/package.json` 和 release tag 一致；`check-plugin-surface.js` 会拒绝漂移。

GitHub workflow 中的外部 `uses:` 必须固定到完整 commit SHA，并保留相邻版本注释；修改 workflow 后运行 `node scripts/check-actions-pinned.js`。

Release/npm 发布自动化必须保持 Trusted Publishing 合同：`release.yml` 使用 GitHub-hosted runner、`id-token: write`、npm >= 11.5.1、无 `NODE_AUTH_TOKEN`、`actions/setup-node` 禁用 `package-manager-cache`，并在文档中记录 npm trusted publisher 的 `release.yml` / `npm publish` 配置。修改 release workflow、`npm/package.json` 或发布文档后运行 `node scripts/check-release-contract.js`。

CI/release job 必须设置 `timeout-minutes`，长耗时测试步骤（Rust tests、coverage）也要有步骤级 timeout，避免 harness 回归占满 GitHub runner。

## 源码结构

```
src/
  main.rs       入口（clap CLI：--surface native-browser|computer，--mode mcp|discover|doctor，--native-config，--image-dir，--omit-inline-images）
  lib.rs        模块声明
  native/mod.rs launch row 发现、环境策略、自有 worker、全双工 MCP 中继、工具策略、CLI/doctor 与进程树清理
  native/tests.rs  native 模块的单元测试（relay、launch 策略、清理与 CLI 结果）
  cli.rs        tools/call/batch 子命令定义
  logging.rs    日志初始化
tests/
  native_process.rs  自有进程树集成测试
  fixtures/          mock native worker
```

## 分支策略

- `main` — 稳定分支，CI 通过才合并
- feature 分支 — `feat/<name>`
- Release tag — `vX.Y.Z` 触发 GitHub Release + npm publish

## 安全红线

- 不在仓库中存放 `.env`、私钥、token
- GitHub Actions 使用 `${{ secrets.* }}`
- 测试数据使用示例域名（`example.com`）
- 不记录或打印 runtime 环境变量的值；诊断只输出布尔存在标记
- 不在文档、示例或日志中写入真实主机名、用户名、私有路径或内部端点
- 不把中继暴露到网络端口；保持 stdio-only
