# 更新日志

本项目的重要变更记录于此文件。每条变更一行，不做硬换行。English version: [CHANGELOG.md](CHANGELOG.md)。

## [Unreleased]

## [3.0.2] - 2026-11-19

### 修复

- 显式 `turn_ended` 收到成功回执后才清空执行 context、轮换默认 turn；拒绝时保留退出清理记录，不让晚到回执清掉后续执行，未确认 hook 报告结果未知且不重放。
- CLI 在 stdout 保留原生 JSON-RPC 错误帧，包括 `code`、`data` 和扩展字段，并以非零状态退出。
- CLI/doctor 校验 initialize 回执的已知 legacy 协议版本，声明 form elicitation 能力以明确返回非交互取消。
- CUA skill 按已安装运行时选择定向入口和清理标签页：已知目标直接进入，释放的用户标签页与既有 persistent 标签页保持打开，区分完成摘要与轮次清理。

### 调整

- 文档明确原生字符串格式与结果保真、CLI 事件和协议边界，以及宿主负责的工具暴露、token 预算与 UI 渲染。

## [3.0.1] - 2026-11-19

### 破坏性变更

- 删除 `--surface browser`、66 个结构化 `codex_*` 工具、旧 CLI 参数，以及扩展适配层的模块、测试和文档。现仅保留 `--surface native-browser`（默认，`node_repl`）与 `--surface computer`（`cua_repl`）；旧工具用户需迁移或保留旧版本。（[升级指南](docs/upgrade-v3.md)）

### 新增

- 新增 macOS arm64/x64 预编译二进制、安装器支持、校验和与 attestation；macOS GUI/TCC 验收仍未完成。
- 新增 Cursor 插件与 MCP Registry manifest、OpenCode/VS Code 模板、Antigravity 配置和 skills.sh 安装说明。
- CLI 新增配合 `--image-dir` 使用的 `--omit-inline-images`，保存截图和制品元数据，同时从 stdout 移除 base64 图片数据。

### 调整

- 内置 skill、接入指南、npm 包和安全说明统一为两个原生运行时入口，工具 schema 与浏览器行为由已安装运行时提供。
- 为中继帧、出站队列、batch 输入和目录分页设置上限，将原生单元测试拆至 `src/native/tests.rs`。
- 发布说明从中英两份 changelog 生成，共用一个安装段落。

### 修复

- 发现结果新增可直接 import 的 `file://` `browser_client_url`，优先选择已安装插件的具体版本而非 `latest` 别名。
- 显式 `turn_ended` 后轮换自动生成的 turn ID，对齐原生浏览器选择顺序与非交互 CLI 的审批取消行为。
- 修正 Windows 发布校验和生成路径，纳入两个已暂存的可执行资产。
- 清除依赖精简后遗留的 Unix libc 直接引用。

## [3.0.0] - 2026-11-19

### 破坏性变更

- Claude Code 插件、客户端示例与 npm 配置现改为分别注册原生 Browser Use（`codex-browser` / `node_repl`）和 Computer Use（`codex-computer` / `cua_repl`），不再默认暴露 66 工具的 `codex_*` 目录。仍需 `codex_*` 工具请显式传入 `--surface browser`。（[升级指南](https://github.com/DeliciousBuding/codex-browser-bridge/blob/v3.0.0/docs/upgrade-v3.md)）

### 新增

- 新增 `--surface native-browser`，直接中继本机 Codex `node_repl` 运行时，复用原生 Computer Use 的启动器、MCP 转发链路与 `discover`/`doctor` 诊断。

### 调整

- 精简内置 skill 为原生优先路由，清理 2.x 遗留参考文档、未使用封装与旧版 `Makefile`。

### 修复

- 解析异常的 `~/.codex/config.toml` 时仅输出行号，避免泄露本机路径或凭据。
- 加固原生 MCP 生命周期：取消后迟到 `tools/list` 仍过滤、`turn_ended` 去重与退出容错、CLI 参数前置校验、断连中断上报及 checkbox 超时处理。

## [2.1.1] - 2026-11-18

### 调整

- 原生 Computer Use 启动器与 Codex CLI 对齐：保留宿主的 surface 与 trusted-service 配置，遵循 `env_vars` 与本地 stdio 环境策略，Windows 命令按构造出的子进程 `PATH` 解析并转发 `PATHEXT`，同时应用配置的启动/工具超时与工具过滤。
- 原生 MCP 中继与 CLI/doctor 统一为同一条 untyped JSON stdio 传输，不引入外部 MCP SDK；CLI 与 doctor 以 `2025-06-18` 协商并默认 30/300 秒启动与工具期限，中继则把握手、分页、elicitation 等待与请求期限交给接收方宿主。
- CLI 与 doctor 跟随有界 `tools/list` 分页，为每次原生工具调用保留关联元数据，并在启动前拒绝未使用的 `--image-dir` 组合。
- Computer Use `js` 使用宿主启动行启用的全部原生 surface，浏览器专用的网络、标签页与输出策略不再包裹该 surface。
- `--mode mcp` 中继的客户端到原生、原生到客户端两个写方向解耦为可取消的出站队列，单向背压不再阻塞反向。
- 扩展浏览器与官方行为对齐：`codex_list_tabs` 支持 `selected_only`，`codex_observe`/`codex_nav_and_wait`/`codex_click_and_wait` 支持 `networkidle` 等待，`codex_wait_for_url` 增加 `url_glob` 与安全 `url_regex`，日志捕获支持 `levels` 过滤，DOM 快照支持同源 `include_iframes`，元素截图保留 padding。

### 修复

- 宿主的 `enabled_tools`/`disabled_tools` 策略现在同时作用于对外目录、直接 `tools/call` 请求与内部 `turn_ended` 清理，中继与 CLI 两种模式一致。
- 原生 CLI/doctor 启动失败时先关闭子进程 `stdin`，清理未确认时保留底层错误链，原生子进程意外关闭 `stdout` 时中继直接失败。
- `input::keypress` 在释放修饰键前重新校验标签页附着，`pointer::scroll` 收敛到单一操作期限，`start_observation` 回滚按连接 epoch 保护。

## [2.1.0] - 2026-11-18

### 新增

- 实验性 `--surface computer` 可接入本机原生 `cua_repl` MCP 服务，支持原生 schema、应用授权询问、图像、元数据、取消与所属 worker 清理。
- 新增原生 CLI `call`/`batch`、运行时 discovery/doctor，以及通过 `--image-dir` 输出图像；非交互 CLI 不会批准应用。

### 修复

- 每次 debugger 附着后启用焦点模拟，与原生 Codex Browser 客户端一致，后台点击、输入与截图不再激活所选标签页或窗口。
- 移除自动 `Page.bringToFront` 兜底，前台激活仍是显式的用户请求工具；焦点配置失败时先分离并报错，再分发输入。
- 附着配置的成功状态绑定到 pipe 代次，重连不会用旧的 debugger 或焦点缓存发送首批输入。
- 外层超时取消调用时移除待处理请求登记，附着恢复可在有界指针释放前完成。
- 保留原生 MCP 扩展字段与宿主服务映射，逐请求校验调用元数据，会话被中断时以非零退出码上报。
- 观察生产者在有界附着/焦点配置完成后才启动，允许有界 detach 回滚，过期配置后不再释放输入。
- 每次原生滚动手势前先定位指针；缺失 mouse-move 确认视为错误，后台滚动超时保持结果未知且不重试。

### 调整

- 按已评审的依赖 PR #52、#53、#54、#55 更新固定版本的 CI actions。
- 因原生 MCP 支持（`rmcp` 3.9.0）将 Rust MSRV 提升到 1.88。
- 桌面验收与源码构建分离发布：Linux ARM64 X11 通过合成应用流程，Windows 输入需要解锁桌面，macOS GUI/TCC 与 Wayland 仍未验证。

## [2.0.3] - 2026-11-17

### 修复

- 自动选择与仅按 family 选择现在要求完整探测；无法证明唯一性时必须给出精确 `browser_id`，因为未探测端点可能属于另一配置文件。
- 忽略已断开的传输与私有 IAB/MCP Apps 回复，同时不把普通超时或权限失败当作陈旧端点；doctor 在端点状态与计数中区分这些情况。

## [2.0.2] - 2026-11-17

### 修复

- `select_browser_from_inventory` 在部分 pipe 或 socket 的 `getInfo` 失败时，仍可选中唯一存活的浏览器扩展。此前 Windows 上 ChatGPT Desktop 自有的 `codex-browser-use-<uuid>` pipe 或 Linux 上残留 `.sock` 会导致自动发现与 `--browser-family` 报 `browser inventory is incomplete`。现在唯一匹配时记录警告并继续，多个存活匹配仍失败关闭，无存活浏览器仍报告 `incomplete`。

## [2.0.1] - 2026-11-16

### 修复

- 接入 ChatGPT 浏览器扩展 1.26+ 不再报 `Cannot read properties of undefined (reading 'timeoutMs')`；扩展在每次 attach 时读取 `params.networkPolicy`，桥接改为发送官方默认的 `{constraints: []}`，不安装任何规则。

### 新增

- `--network-policy`、`CODEX_BRIDGE_NETWORK_POLICY` 与 `network_policy` TOML 键可设置每次 attach 安装的 Browser Use 网络策略，并在 detach 前按扩展预算校验（128 条约束、4096 个模式、64 KiB 模式文本、1–30000 ms 超时），策略非法时直接失败而不破坏已附着标签页。

### 调整

- 官方插件基线更新到 `26.1111.21308`（23 个接口、153 个成员），`check-official-drift.js` 同时统计两种命令注册打包方式。

## [2.0.0] - 2026-10-25

已有 claim、配置与续接文件的迁移见 [docs/upgrade-v2.md](docs/upgrade-v2.md)。工具名称与数量不变。

### 调整

- CLI 续接文件升级为 v2，绑定精确扩展身份并拒绝旧版或畸形 token；OS 级锁拒绝并发使用，状态在操作前保存，断连不确定后仍可恢复。
- `codex_claim_tab` 要求来自当前用户标签页枚举的预期标题与 URL，拒绝纯数字调用；原生 claim 缺少原子快照比较，快照变化时只报告可能存在所有权变更，不重放。
- 显式配置错误与未知键直接失败；浏览器选择器采用一致的 CLI/环境变量/配置文件优先级；诊断输出区分工具 profile、已附着实例身份与受权限约束的能力。

### 修复

- 不完整探测无法再证明隐式唯一浏览器；rebind/finalize 排除在途工具，未解决的旧轮次仍可恢复，同一实例重新绑定保留原轮次。
- 原生显式无路由响应按陈旧端点计数，而非未知浏览器实例；超时与权限失败仍阻止隐式选择。

## [1.13.0] - 2026-10-24

### 新增

- `codex_observe`：有界的标签页/文档事件日志，含游标缺口、frame/loader 状态、活动 dialog/chooser ID 与持久化渲染日志，采集在动作前开始。
- `codex_file_chooser`：显式拦截、不透明且绑定文档的句柄、多选模式与有界同源上传。
- Locator 支持 and/or/has/has_not 组合、有界规格与受限安全正则、精确 select 描述、trial/force、顺序输入、可信 `setChecked` 与同目标异步后置条件且不重放。
- 截图支持有界 clip/scale，网络支持 URL 子串过滤，并提供本地 MCP 指南资源。

### 修复

- 指针与键盘输入改为先激活后台标签页再分发：Chrome 会对被遮挡标签页确认合成输入却不实际投递，导致点击报成功但页面未变化。设置 `CODEX_BRIDGE_NO_AUTO_FOREGROUND=1` 可恢复旧行为。
- 扩展报 `Debugger unattached` 时清除缓存的附着状态，悬挂窗口超时同样使缓存失效，输入清理使用的短期限则有意不失效。
- `codex_navigate`、`codex_nav_and_wait` 与 `codex_wait_for_load` 会报告 `net::ERR_*` 导航结果与浏览器错误页，不再返回成功。
- 键盘事件与 `Input.insertText` 走不重放的 CDP 路径，附着恢复不会重复 keydown 或插入文本。
- `codex_hover` 在报告成功前确认目标进入 `:hover` 状态。
- `codex_console_logs` 与 `codex_network_monitor` 解开扩展的 `onCDPEvent` 封装，并在字节统计前把采集范围限定到目标标签页。

### 调整

- 导出与页面资源共用有界的获取/输出预算，使用严格显式目录、暂存与原子不覆盖发布；缺少原子语义的文件系统直接失败，不做破坏性兜底。
- 导航组合动作在动作前布置并在 commit、DOMContentLoaded 或 load 后返回；导航、dialog 与文件变更在歧义错误下不重放。
- `call` 与 `batch` 默认在退出时结束浏览器轮次（含失败路径），`--keep-turn` 可退出该行为，`--session-file` 可在多个命令间保持同一轮次；Ctrl-C 与 SIGTERM 遵循同样策略。
- `--session-file` 在 `call`/`batch` 之外被拒绝，`--finalize` 与 `--keep-turn` 冲突；在拨号前被拒绝的命令不再创建幽灵会话文件或删除既有文件。
- mock 传输客户端测试（读循环、重连、事件路由）现在在 Windows 与 Linux 上同时运行。
- `codex_browser_management` 对会移除或丢弃浏览器状态的方法（`remove`、`delete`、`clear`、`close`、`discard`）要求 `confirm=true`。
- Unix 上 `CODEX_BRIDGE_SOCKET_DIR` 为显式覆盖，`$XDG_RUNTIME_DIR/codex-browser-use` 先于 `/tmp` 尝试，返回的 socket 路径标明其来源目录。
- 配置优先级与文档一致：CLI 参数 > 环境变量 > 配置文件 > 默认值。

## [1.12.0] - 2026-10-24

### 新增

- 共注册 64 个 MCP 工具（`basic` 45 / `network` 63 / `full` 64）：`codex_locator` 支持严格的 CSS/role/text/label/placeholder/test-id 读取与操作，`codex_cua_move` 支持指针移动，坐标与后端节点点击支持双击。
- CUA 指针参数对齐官方 surface（`button` 1..5、`keypress`/`keys` 修饰键、`scrollX`/`scrollY`、拖拽 `path[]`），由单一不重放的 CDP 分发器执行，部分按下/移动失败后尝试释放；CSS `codex_click` 与 `codex_hover` 改用 locator 命中测试，滚动走 `Input.synthesizeScrollGesture`。
- Shell `tools`、`call` 与同会话 `batch` 命令复用 MCP profile 与处理器，单结果默认 16384 字节，图像自动摘要，`--finalize` 在失败后仍尝试清理轮次。
- 支持 Linux `/tmp/codex-browser-use/*.sock` 发现，并校验 socket 所有者/权限、目录与对端 UID；doctor 增加传输类型与路径，同时保留旧 `pipe_*` 字段。
- 新增 Linux GNU x64/arm64 原生 CI 与发布任务及 npm 包选择（发布二进制要求 glibc >= 2.39），Windows 资产名保持稳定。
- 新增浏览器实例发现与精确绑定：`codex_list_browsers` 报告稳定实例 ID、浏览器 family、配置文件元数据、能力与传输方式，`codex_bind_browser` 固定单个实例，多实例选择失败关闭，重连绑定到实例。
- 对齐官方的浏览器历史、Chromium 管理、视口覆盖、可见性、标签页交接/交付标记与整页截图能力。
- 新增 `docs/official-parity.md` 与 `scripts/check-official-drift.js`，跟踪内置 Codex Browser 插件 API、命令目录、包清单与扩展 family 目录的变化。

### 调整

- 在 `docs/official-parity.md` 中区分桥接自有 surface 与官方 API：CUA 输入沿用官方字段名，AX 与后端 DOM ID 保持独立，剪贴板、WebMCP、页面资源、导出与 unrestricted evaluate 逃生口保留显式桥接契约。
- `basic` 与 `network` profile 纳入发现/绑定、严格 locator、历史、管理、视口、可见性、交接标记、剪贴板、WebMCP 与内容导出，低 token 客户端也能使用新入口。

### 修复

- 交互式 CLI `try` 命令保留原始 JSON 参数字符串。
- doctor 输出为每条连接纳入浏览器 family、扩展实例 ID、扩展 ID 与配置文件名称。
- 浏览器重绑定会关闭旧传输，存在归属标签页时要求此前 finalize 或显式 `end_previous_turn=true`。
- `codex_finalize` 发送官方 `turnEnded` 并保留已标记的交接/交付标签页；pipe 已断开不算成功清理。
- 无障碍查询返回后端 DOM `node_id`、独立 AX ID 与 `actionable` 标记，仅 AX 的节点不再传入 DOM 操作。
- 连接校验与后续请求使用相同的 session 与 turn ID。
- doctor 将陈旧传输名称说明与健康告警分离，并解释同 family 实例歧义。

### 安全

- 更新 `tokio` 至 `1.53.3`，处理 `RUSTSEC-2026-0386`。

## [1.11.4] - 2026-10-22

### 调整

- Claude 插件改为通过固定版本的 `npx` 入口启动 npm 包，一条命令的 marketplace 安装不再依赖预先全局安装 npm 包；全局安装仍作为快速路径，并仍为手工 MCP 示例所需。
- README、架构、贡献、发布与示例文档统一描述插件与二进制边界：插件提供 skill 与 MCP 配置，固定版本的 npm 包在首次使用时提供 Windows 二进制。
- 插件表面检查在两侧 README 中强制 `npx` 版本固定与一键安装命令串。

## [1.11.3] - 2026-10-21

### 调整

- 桥接仍为通用 MCP server，保留完整 52 工具面与双纪元 MCP 支持；Claude Code 仍是主要插件分发路径，支持渐进式 skill 加载与工具搜索延迟。
- 安装提示改为引导 Codex / ChatGPT desktop 用户使用其原生浏览器集成，不再安装重复的桥接插件。

### 移除

- 移除可移植 Agent Plugins / OpenAI manifest、`.codex-plugin/` 兼容 manifest、仓库级 Codex marketplace 及相关安装说明。

## [1.11.2] - 2026-10-21

### 新增

- 面向 Claude Code、ChatGPT desktop 与 Codex 的可安装插件分发：提供可移植 Agent Plugins manifest、Codex 兼容 manifest、Claude marketplace 与 Claude 插件 manifest，同时保留完整 52 工具 MCP server。
- Claude Code MCP 工具搜索集成：仅 `codex_doctor` 标记 `anthropic/alwaysLoad`，其余 schema 保持延迟加载。
- agent skill 改为渐进式 bundle：`SKILL.md` 负责路由与核心工作流，四个任务 reference 按需加载。

### 调整

- 运行时诊断、包元数据、架构、贡献、安全与 agent skill 措辞对齐当前的 ChatGPT desktop 应用与浏览器扩展命名。
- 安装器与 doctor 建议的 MCP 配置改为推荐 34 工具的 `basic` profile；未配置时内置默认仍为 `full`，支持工具搜索的客户端插件安装默认 `full`。
- npm 安装提示优先推荐 Claude Code 插件，其次 ChatGPT desktop / Codex marketplace，skill 复制与手工 MCP JSON 作为兜底。
- 合并本轮固定的 GitHub Actions 更新：`actions/attest` 4.2.2、`codecov/codecov-action` 7.1.1、`taiki-e/install-action` 2.87.21。

### 修复

- 插件打包复用同一个根 `mcp.json`，Claude 不再自动加载第二份项目级副本导致同一 MCP 端点启动两次。
- live E2E harness 并发排空 doctor 的 stdout/stderr，不再在读取前等待管道缓冲写满。
- Windows npm 命令 shim（`.cmd`、`.bat` 与无扩展名 shim）在指向不同安装路径时仍计为可启动。
- doctor 的 pipe 诊断报告当前浏览器扩展元数据，不再留空浏览器标签。

## [1.11.1] - 2026-08-19

### 新增

- npm 安装器下载发布二进制时遵循 `HTTPS_PROXY` / `https_proxy`（`ALL_PROXY` 兜底、`NO_PROXY` 排除），以 HTTP CONNECT 隧道通过代理，修复只能经代理访问 GitHub 的机器上 `npm i -g` postinstall 失败的问题。

### 说明

- 无 Rust 代码变更，二进制重建仅包含版本号更新。

## [1.11.0] - 2026-08-19

### 新增

- 支持 MCP 协议修订 `2026-07-28`（双纪元 server）：旧纪元（`2024-11-05` … `2025-11-25`）保留经典 stdio 生命周期并协商版本，线格式与 v1.10.x 逐字节一致；携带 `params._meta["io.modelcontextprotocol/protocolVersion"]` 的新纪元请求无需 `initialize` 即可无状态应答，结果包含 `resultType`、`_meta` 中的服务身份与 `ttlMs`/`cacheScope` 缓存提示；不支持的版本返回 `-32022`。
- `server/discover` 公布支持的版本、能力与说明，且无版本元数据时也应答，便于新纪元客户端在 stdio 上探测兼容性；`ping` 仅对旧纪元客户端应答。
- 全部 52 个工具加入 MCP 工具注解（`readOnlyHint`、`destructiveHint`、`idempotentHint`、`openWorldHint`），其中 `openWorldHint` 恒为 true，因为每个工具都触及真实浏览器。
- `initialize` 结果包含描述当前桥接用法的自然语言 `instructions`。

### 调整

- 新增 `src/mcp/lifecycle.rs`，负责版本协商、纪元判定、新纪元结果封装与规范错误信封。
- 已对照官方 `2026-07-28` schema 与 Codex v0.148.x 的扩展能力（`viewport`、`pageAssets`、`browserTabMentions` protocol v1）核验。

## [1.10.1] - 2026-07-14

### 修复

- `codex_evaluate` 在 `Runtime.evaluate` 上设置 `awaitPromise=true`，`fetch(...).then(...)` 等异步表达式返回真实值而非空 Promise 对象。
- `codex_evaluate` 把 `exceptionDetails` 作为工具错误上报，不再静默返回部分结果。

### 文档

- 在 README（EN/ZH）与打包的 agent skill 中记录 `codex_evaluate` 的 Promise 等待与异常行为。

## [1.10.0] - 2026-07-10

### 调整

- `codex_file_input` 要求显式 `CODEX_BRIDGE_UPLOAD_BASE`，不再回退到进程工作目录。
- npm 预发布版本发布到 `next` dist-tag，稳定版本保持 `latest`。
- 导航 URL 仅允许 `http://` 与 `https://`，发送 CDP 前统一规范化，并拒绝含原始空白、控制字符、反斜杠或缺主机的地址。
- MCP 等待、采集与表单填写延迟加入有界时长校验，`codex_doctor` 纳入 `basic` profile。
- 通用 CDP 执行收敛到显式低层方法 allowlist，阻止绕过专用安全封装的原始方法。

### 修复

- MCP stdio 行读取加入长度上限，cookie 工具 URL 过滤与导航采用相同的 scheme 边界。
- 页面资源内容获取在既有大小超出字节预算时跳过，加入获取超时，并统一未知大小与已获取内容的处理。
- GitHub Release 资产先经可恢复的 draft/更新流程，并在 npm 发布前公开 GitHub Release，使 npm 安装可立即下载公开二进制。
- 发布任务校验精确的注释 tag commit SHA，并要求带日期的 changelog 段非空。
- 发布、CI 与供应链 workflow 的 actions 固定到完整 commit SHA，新增 CI 强制 pin 扫描与 agent 表面漂移检查。
- 发布校验在 changelog 段为空时提前失败；npm 发布重跑仅在预期 dist-tag 正确时跳过已发布版本。
- 增加 workflow 并发控制，取消过期 PR 校验并串行同 tag 发布。
- 标签页关闭、finalize 与附着失败时回收每标签页客户端锁，非法页面资源大小按未知处理，避免长会话积累陈旧或误导状态。
- `codex_print_pdf` 改为有界 CDP 流式输出并显式关闭流，不再在内存中持有无限 base64。
- 异步排空 E2E stderr，并用 CI 假桥接超时 harness 覆盖超时路径。
- npm 包内置多客户端 MCP 配置示例，发布脚本仅保留 `postinstall`，发布前校验内置 skill，并在 tarball 中纳入 README 与 LICENSE。
- 显式配置路径具有权威性，拒绝相对上传路径，页面资源超时后清理待处理 CDP 请求，获取内容统一为 base64。
- 更新 `crossbeam-epoch` 清除 dev 依赖基准链路的 `RUSTSEC-2026-0204`，并把 Dependabot 与 Rust 1.85 MSRV 对齐，暂缓需要更新 Rust 的 `criterion` minor 升级。

### 文档

- 记录发布/tag/changelog/npm 来源策略、多客户端配置示例、上传基目录配置、profile 行为与数字标签页 ID schema。

## [1.9.1] - 2026-06-21

v1.7.0 之后的首个版本。工具数从 28 增至 52，agent 不只能驱动页面，也能观察页面发生了什么。

### 新增

- 事件订阅层把服务端推送的 CDP 事件路由给订阅者：`codex_network_monitor` 以 `[{url, method, status, mime_type}]` 采集请求↔响应对，`codex_console_logs` 采集 `console.*` 输出。
- `codex_bring_to_front` 通过 `Page.bringToFront` 激活后台标签页；sticky CDP 调用使用 20 秒预算而非共享 60 秒总时长，被节流标签页快速失败。
- `codex_wait_for_element`、`codex_wait_for_url`、`codex_hover`、`codex_select_option`、`codex_drag`、`codex_storage`、`codex_get_url`、`codex_get_title`、`codex_screenshot_element`、`codex_print_pdf`、`codex_performance_metrics`、`codex_delete_cookies` 与 `codex_emulate_device`。
- `codex_screenshot` 支持 `format`（png/jpeg/webp）与 `quality`，JPEG 体积明显小于 PNG。
- `codex-browser-bridge --mode doctor` 以 JSON 输出 pipe 健康、Chrome 版本与延迟，无需 agent 参与。

### 修复

- 事件订阅此前只把 `params` 传给订阅者，`network_monitor` 无法区分事件类型；现在传完整帧。

### 架构

- 读循环把带 `method` 且无 `id` 的帧路由给订阅者而非丢弃，对外暴露 `subscribe_events`/`unsubscribe_events`；网络监控按 `requestId` 配对请求与响应事件，输出干净列表。
- sticky 附着静默超过 20 秒视为标签页被后台节流，改为触发完整重附着，而不是等满 60 秒。

### Profile

`basic` 33 / `network` 50 / `full` 52。

## [1.9.0] - 2026-06-20

### 新增 — 8 个 MCP 工具（28 → 36）

- `codex_file_input`：通过 `DOM.setFileInputFiles` 向 `<input type=file>` 上传文件，使用 `canonicalize` + 前缀检查防路径穿越，限制 10 MB 与普通文件，可用 `CODEX_BRIDGE_UPLOAD_BASE` 配置。
- `codex_dialog`：通过 `Page.handleJavaScriptDialog` 处理 alert/confirm/prompt，支持接受/拒绝与可选提示文本。
- `codex_find_element`：在 AX 树中按 ARIA role 和/或可访问名称查找元素，返回可与 `codex_click_element` 配合的节点 ID。
- `codex_click_element`：通过 `DOM.resolveNode` → `DOM.getBoxModel` → 输入分发按节点 ID 点击，不注入 JS。
- `codex_nav_and_wait` 与 `codex_click_and_wait`：导航/点击加等待加载的组合调用。
- `codex_form_fill`：按 `{selector: value}` 映射批量填写表单，可选提交。
- `codex_doctor`：枚举 pipe、探测连通性并报告延迟与浏览器版本的自诊断。

### 调整

- `mcp.rs` 拆分为 `src/mcp/{mod,types,schema,handlers}.rs`，各自不超过 400 行；新增集中式 `src/security.rs` 提供 `validate_url` 与 `validate_file_path`。
- 工具 profile：`basic`（25）/ `network`（32）/ `full`（36），可通过 `CODEX_BRIDGE_PROFILE` 或 `--profile` 选择；`validate_url` 迁入 `security.rs`，`Tool` 可见性放宽为 `pub(crate)`，新增 `Server::new_with_profile()`。

### 文档

- ROADMAP.md 更新 v1.9.0 完成状态与 SUPER 评分。

## [1.8.0] - 2026-06-20

### 新增

- CDP 错误规范化：Target closed 等 CDP 级错误通过新增的 `BridgeError::Cdp`（携带方法名与错误码）体现为 `isError: true`。
- sticky attach：按标签页缓存 CDP 会话，跳过重复的 detach+attach，同一标签页的连续调用 RTT 约下降 50%。
- `codex_network_cookies` 默认脱敏 cookie 值（`redact_values: true`），`codex_network_set_cookie` 强制 URL 校验。
- `codex_execute_cdp` 使用方法 allowlist，阻止 Browser/Debugger/Target/Emulation/Security/Tracing 域；`codex_page_assets` 暴露扩展 `pageAssets` 能力并支持按 frameId 获取资源。

### 调整

- 全部 28 个工具加入分组标签（`[Tabs]`、`[Navigation]`、`[DOM]`、`[Page]`、`[Input]`、`[CDP]`、`[Network]`、`[Session]`）、交叉引用与更清晰的 LLM 指引。
- `fullPage` 改为 `full_page`（保留兼容回退），`timeout_ms` 改为整数类型，CLI 从 `browser.rs` 迁至 `src/cli.rs`。

### 移除

- 移除过度设计的 `BridgeClient` trait（无 mock 使用方）、Go 遗留代码（`internal/`、`cmd/`、`go.mod`、`go.sum`、`.golangci.yml`，共 −5382 行），并以八条模式的 `is_session_invalid_error` 取代 `is_debugger_error`。

### 性能

- `encode_frame` 把长度头与负载合并为单次 `write_all`；MCP stdio 使用 `BufReader::read_until` 与可复用缓冲；sticky attach 命中缓存时不再 detach+attach。

### 安全

- CDP 错误消息在对外前去除 `\n` 与 `\r`，会话缓存在 `finalize` 时清理、出错时自愈，cookie 值默认脱敏。

### 文档与治理

- 增加 15 个 GitHub topics、启用 Discussions、设置 homepage，npm 增加 keywords 与 `homepage`/`bugs`，并提供 claude-code、openclaw、hermes-agent 与 cursor 的 `examples/` 模板。
- CI 与发布流程加入 `cargo clippy -D warnings`，覆盖率改用 `cargo-llvm-cov`，`.gitignore` 覆盖 `docs/ask-gpt/`。
- 同步 README.zh-CN，按 Rust-only 工作流重写 CONTRIBUTING.md，修正 SECURITY.md 过期路径，ROADMAP.md 更新 v1.7.0–v2.0.0 计划。
- 外部评审：SUPER 多维评审将 CDP 错误规范化与 sticky attach 评为 PASS_WITH_FIXES，五项必改项已全部解决；3-agent 架构评审与独立的 ChatGPT 审计确认了架构方向与优先级。

## [1.7.0] - 2026-06-20

### 新增

- `codex_execute_cdp`：通用 CDP 执行器，可传任意方法与参数，是覆盖全部 CDP 域的逃生口。
- `codex_page_assets`：通过 `Page.getResourceTree` 列出页面资源，可选经 `Page.getResourceContent` 获取 base64 内容。
- `codex_network_cookies`（支持 URL 过滤的 `Network.getCookies`）与 `codex_network_set_cookie`（支持标准 cookie 属性）。

### 调整

- MCP 工具数从 24 增至 28，浏览器 helper 增加 `execute_cdp_generic`、`get_resource_tree`、`get_resource_content`、`get_cookies` 与 `set_cookie`。

### 测试

- 新增带 mock pipe 基础设施的 `tests/cdp_tools_e2e.rs`、7 个 e2e 测试与 4 个 schema 校验测试，测试总数从 85 增至 92。

### 治理

- AGENTS.md 记录 MCP 工具设计模式与安全规则，公开仓安全审计未发现密钥、token 或敏感数据。

## [1.6.0] - 2026-06-09

### 新增

- 桥接二进制改用 Rust 重写，覆盖扩展线协议、pipe 发现、MCP 工具、CLI 命令与浏览器 helper。
- CI 增加 Windows Rust 重写任务，运行 `cargo check`、`cargo test` 与 x64/arm64 release 模式构建。
- 发布校验比对 `Cargo.toml` 与发布 tag，并从 tag 对应的 changelog 段提取说明。
- Rust 重写文档说明 npm 兼容计划与本地构建命令。

## [1.5.4] - 2026-06-09

### 修复

- Chrome 报告目标已消失时，`codex_close_tab` 回收该标签页的 CDP 锁。

## [1.5.3] - 2026-06-09

### 修复

- 发布流程在创建 GitHub Release 资产后发布 npm；手工发布先检出目标 tag 再执行测试、lint、构建与发布；npm 发布前校验 `npm/package.json` 与 tag 一致。
- npm 安装器优先使用包内置的二进制校验和，发布校验和文件保留为旧包与开发覆盖的兜底。
- CUA 点击、输入与按键把每个高层输入序列置于同一标签页锁下，并在序列内重试 debugger detach 错误；`codex_wait_for_load` 按调用方超时约束每个 CDP 请求。
- 标签页成功关闭与会话 finalize 后回收 CDP 标签页锁。
- README、SECURITY 与 issue 模板说明分享输出前必须脱敏的本地浏览器数据。

## [1.5.2] - 2026-06-09

### 修复

- 发布 tag 在创建 GitHub Release 资产前先运行测试与 lint，CI 运行安装器校验和解析的 Node 测试。
- 公开 issue 与贡献文档要求上报者脱敏本地浏览器数据，忽略规则覆盖生成的覆盖率文件、npm pack 输出与包归档。
- npm 安装器默认按包版本下载对应发布资产，开发用下载覆盖需设置 `CODEX_BRIDGE_ALLOW_DEV_DOWNLOADS=1`。
- README 直接说明项目关系与浏览器会话权限。

## [1.5.1] - 2026-06-09

### 修复

- 同一标签页的 CDP detach、attach 与 execute 串行化，避免并发操作互相分离。
- `codex_click` 在选择器缺失时报错，`codex_dom_click` 拒绝过短的 `DOM.getBoxModel` 数组而非 panic，`codex_wait_for_load` 在超时前重试导航期的瞬时 CDP 错误。
- pipe 发现忽略裸 `codex-browser-use` 命名空间条目；MCP JSON-RPC 处理忽略通知、拒绝畸形信封与零长度帧，并在打开 pipe 调用前校验必需工具参数。
- npm 包使用提交进仓库的 JavaScript 命令包装器，按包版本而非 `latest` 下载资产，拒绝不支持的 CPU 架构，并在 CI 中检查包内容。
- 手工发布流程要求合法的 `v*` 发布 tag。

## [1.5.0] - 2026-06-05

### 修复

- 修复 Codex Desktop 26.602+ 之后的 pipe 发现：新版本创建的 pipe 名带反斜杠分隔符（`codex-browser-use\<uuid>`），PowerShell 的 `Get-ChildItem` 会当作目录跳过，改用 `[System.IO.Directory]::GetFileSystemEntries` 并手工剥离前缀以兼容两种格式。
- `extractUUID` 同时剥离前缀后的 `-` 与 `\` 分隔符；pipe 告警阈值从 >1 提升到 >2，因为 Codex 升级期间新旧格式可能共存。

## [0.3.0] - 2026-05-19

### 修复

- 严重：MCP 读缓冲从 4 KB 默认值提升到 10 MB，`notifications/initialized` 不再返回错误，修复 CLI 纯空白输入 panic 与 EOF 忙循环，fill 在元素不存在时报错而非静默成功。
- 高：全部 19 个 MCP 工具处理器检查 `json.Unmarshal` 错误，Click/Fill 的 JavaScript 字面量改用 `json.Marshal` 而非 `%q`，`Response.ID` 改为 `*int` 以免 `id:0` 被误判为通知，`writeResult`/`writeError` 的序列化错误纳入检查。
- 高：历史导航校验两个数组边界，PowerShell pipe 发现子进程加 15 秒超时，`readLoop` 不再在重复响应上阻塞发送。
- 中：`CUAType` 按 CDP 规范在单次 debugger 附着后分发 keyDown+char+keyUp，健康检查超时改为 5 秒，CLI `try` 从 `args[2:]` 提取 JSON，DOM 快照兜底加标记，box model 访问前检查数组长度。
- 低：`newUUID` 返回错误并用 `math/rand` 兜底而非 panic，`BRIDGE_DEBUG_LOG` 打开失败记录警告，`os.Exit` 移入 `main()` 以便 deferred 清理执行，`extractUUID` 条件性剥离单个分隔符，定时器在 defer 中停止，claim 自动附着错误写入日志。
- 低：补充 `SendNotification` 测试，wait-for-load 超时测试以 `strings.HasPrefix` 取代脆弱切片，E2E 截图测试校验 base64 非空。

### 安全

- URL 校验阻止 `file:`、`javascript:`、`data:` 与 `vbscript:` 协议；存在多个 `codex-browser-use-*` pipe 时给出命名管道伪造警告；错误消息记录前去除换行；JSON 转义兜底返回 `""` 而非把非法 JSON 传入求值脚本；补充缺失的 `ping` 处理器。
- 测试 mock 强制精确方法匹配并检测重复响应。

### 调整

- `codex_dom_get_visible` 明确返回不含节点 ID 的可读 DOM 树（节点 ID 由 `codex_dom_snapshot` 提供），`codex_screenshot` 标注 `fullPage` 为保留参数、当前仅截取视口。

## [0.2.0] - 2026-05-16

### 新增

- `codex_navigate_back` 与 `codex_navigate_forward` MCP 工具，暴露客户端已有的历史导航。
- `codex_wait_for_load` 轮询 `document.readyState` 直到 `complete` 或超时；`codex_screenshot` 返回 MCP `image` 内容，agent 可直接查看截图。
- `MCPServer.SetVersion` 把构建版本带入 MCP `initialize` 握手，`NewMCPServerWithIO` 支持可测试的 I/O 注入。
- 覆盖 `protocol`、`client`、`discovery` 与 `mcp` 的单元测试：内存 `net.Pipe` 假服务端、`-race` 下的并发 `SendRequest` 压力测试、线格式不变量与完整的 client → MCP 处理器测试；CI 运行 `go test -race -cover`。

### 修复

- `discovery.extractUUID` 不再截断含连字符的 UUID，pipe 未找到与拨号失败的错误信息更清晰，`Makefile install-local` 在 Windows 复制 `.exe`，清理 README 中重复的安装选项编号。

### 内部

- 新增包装既有 `net.Conn` 的 `client.NewFromConn`（供测试使用），忽略 `cover.out` 与 `*.coverprofile`。

## [0.1.0] - 2026-05-16

### 新增

- `codex-browser-use-*` 命名管道发现与 go-winio 连接。
- 会话管理（`createTab`、`getTabs`、`getUserTabs`、`claimUserTab`、`closeTab`）与 CDP 导航（`Page.navigate`、`Page.reload`、`Page.getNavigationHistory`）。
- CDP 截图（base64 PNG）、无障碍 DOM 快照、JavaScript 求值、选择器点击/填充，以及 `Input.dispatchMouseEvent`/`Input.dispatchKeyEvent` 的 CUA 输入。
- 20 个工具的 MCP server（stdio JSON-RPC）、交互式 CLI 调试模式与列出活动 pipe 的 discover 模式。

### 说明

- 线协议使用 camelCase 方法名（`getInfo`，不是 `get_info`），`executeCdp` 要求嵌套 `{target: {tabId}}` 格式，任何 CDP 命令前必须 `attach`，每个 pipe 连接都会创建新的浏览器会话。
