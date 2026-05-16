<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    让 Claude Code 和其他 MCP Agent 通过 Codex Desktop 的浏览器桥接层控制你现有的 Chrome 浏览器。
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
  <a href="README.md">English</a>
</p>

---

`codex-browser-bridge` 是一个小巧的 Go 二进制文件，将 Codex Desktop 的 Chrome 浏览器桥接层作为 MCP 服务器暴露出来。

它连接本地 Codex 浏览器的 named pipe，使用相同的 length-prefixed JSON-RPC 协议，为 Claude Code 或任何兼容 MCP 的 Agent 提供浏览器控制工具。

## 为什么需要这个

Codex Desktop 通过本地 named pipe 与其 Chrome 扩展通信。其他 Agent（例如 Claude Code）无法直接访问这个内部桥接层。

本项目复用你机器上已经存在的浏览器桥接能力，并将其包装为 MCP 服务器。

Agent 因此可以：

- 查看你当前的浏览器标签页
- 接管（claim）现有标签页
- 创建和关闭标签页
- 导航页面
- 截图
- 读取 DOM / 无障碍树快照
- 点击、输入、滚动、执行 JavaScript

适用于需要真实浏览器会话的场景，例如仪表盘、已登录的 Web 应用、本地开发服务器或文档站点。

## 状态

实验性。

当前版本专为本地 Windows 环境设计，需要已安装并运行 Codex Desktop 和 Codex Chrome 扩展。

## 特性

- stdio 上的 MCP 服务器
- 单个 Go 二进制文件
- 无需复制浏览器配置文件
- 使用你现有的 Chrome 会话
- 自动发现 `codex-browser-use-*` named pipes
- 通过 JSON-RPC 与 Codex Desktop 的 extension host 通信
- 使用 Chrome DevTools Protocol 命令控制页面
- 包含交互式 CLI 模式用于调试

## 环境要求

- Windows
- Chrome
- Codex Desktop 正在运行
- Codex Chrome 扩展已安装并启用
- Go 1.26+（仅从源码构建时需要）

> 桥接器连接 Codex Desktop 创建的本地 named pipe。如果找不到 pipe，请先启动 Codex Desktop 并确保扩展已激活。

## 安装

### 方式一：npm install

```bash
npm i -g @delicious233/codex-browser-bridge
```

### 方式二：Go install

```bash
go install github.com/DeliciousBuding/codex-browser-bridge/cmd/bridge@latest
```

确保 Go 的 bin 路径在 `PATH` 中。

### 方式三：下载 Release

从以下地址下载最新二进制文件：

```text
https://github.com/DeliciousBuding/codex-browser-bridge/releases
```

将 `codex-browser-bridge.exe` 放到 `PATH` 中的任意位置。

### 方式三：从源码构建

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
make build
```

生成的二进制文件位于：

```text
bin/codex-browser-bridge.exe
```

## Claude Code 快速上手

将 MCP 服务器添加到 Claude Code 的设置文件中。

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["-mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

如果是从源码构建，使用绝对路径：

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "D:/path/to/codex-browser-bridge/bin/codex-browser-bridge.exe",
      "args": ["-mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

编辑设置文件后请重启 Claude Code。

然后可以这样使用：

```text
列出我当前打开的浏览器标签页。
```

```text
打开 https://example.com 并截图。
```

```text
接管我当前的文档标签页，总结页面内容。
```

## CLI 用法

该二进制有三种模式。

### MCP 模式

默认模式，供 Claude Code 或其他 MCP 客户端使用。

```bash
codex-browser-bridge -mode mcp
```

### Discover 模式

列出活跃的 Codex 浏览器 named pipe。

```bash
codex-browser-bridge -mode discover
```

### 交互式 CLI 模式

无需 MCP 客户端即可调试桥接器。

```bash
codex-browser-bridge -mode cli
```

连接指定 pipe：

```bash
codex-browser-bridge -mode cli -pipe "codex-browser-use-<uuid>"
```

可用 CLI 命令：

```text
tabs          create        close <id>    user-tabs     claim <id>
nav <id> <url>   snapshot <id>   screenshot <id>
info          ping          try <method> [json]   quit
```

## MCP 工具

### 标签页管理

| 工具 | 说明 |
|------|------|
| `codex_list_tabs` | 列出当前桥接会话管理的标签页 |
| `codex_create_tab` | 创建新标签页 |
| `codex_close_tab` | 关闭标签页 |
| `codex_user_tabs` | 列出浏览器所有打开的标签页 |
| `codex_claim_tab` | 接管现有用户标签页进行自动化 |

### 导航

| 工具 | 说明 |
|------|------|
| `codex_navigate` | 导航到指定 URL |
| `codex_reload` | 重新加载标签页 |

### 页面检查

| 工具 | 说明 |
|------|------|
| `codex_screenshot` | 截取 base64 PNG 截图 |
| `codex_dom_snapshot` | 获取无障碍树快照 |
| `codex_dom_get_visible` | 获取简化版可见 DOM 树 |
| `codex_evaluate` | 在页面上下文中执行 JavaScript |
| `codex_get_info` | 获取扩展后端信息 |

### 交互

| 工具 | 说明 |
|------|------|
| `codex_click` | 通过 CSS 选择器点击元素 |
| `codex_fill` | 通过 CSS 选择器填充表单输入 |
| `codex_dom_click` | 通过节点 ID 点击 DOM 节点 |
| `codex_cua_click` | 通过屏幕坐标点击 |
| `codex_cua_type` | 在当前焦点处输入文本 |
| `codex_cua_keypress` | 按下键盘按键 |
| `codex_cua_scroll` | 按坐标滚动 |

### 会话

| 工具 | 说明 |
|------|------|
| `codex_name_session` | 为浏览器会话设置名称 |
| `codex_finalize` | 结束会话并清理标签页 |

## 架构

```text
MCP 客户端
  Claude Code / 其他 Agent
        │
        │ stdio JSON-RPC
        ▼
codex-browser-bridge
  Go 二进制文件
        │
        │ length-prefixed JSON-RPC 帧
        ▼
Windows Named Pipe
  \\.\pipe\codex-browser-use-*
        │
        ▼
Codex Desktop extension host
        │
        ▼
Codex Chrome 扩展
        │
        ▼
Chrome 标签页
```

## 工作原理

1. 桥接器搜索匹配 `codex-browser-use-*` 的本地 named pipe。
2. 通过 `go-winio` 连接到选定的 pipe。
3. 每个请求编码为 4 字节小端长度前缀 + JSON-RPC 载荷。
4. 浏览器操作被发送到 Codex extension host。
5. 页面级操作使用 Chrome DevTools Protocol 命令，如 `Page.navigate`、`Page.captureScreenshot`、`Runtime.evaluate` 和 `Input.dispatchMouseEvent`。
6. MCP 层将这些操作暴露为 `codex_*` 工具。

## 安全说明

本工具赋予 Agent 访问你活跃浏览器会话的能力。

请像使用其他浏览器自动化工具一样谨慎使用：

- 不要将桥接器暴露到网络端口
- 不要为不受信任的 MCP 客户端运行
- 在允许敏感操作前检查 Agent 的行为
- 避免在包含密码、支付信息、私有令牌或生产管理后台的页面上使用
- 请记住被接管的标签页可能已经登录

本项目仅用于本地开发和受控自动化。

## 故障排除

### 找不到 pipe

```text
No codex-browser-use pipes found. Is Codex Desktop running?
```

检查：
- Codex Desktop 是否正在运行
- Chrome 是否正在运行
- Codex Chrome 扩展是否已安装并启用
- 扩展是否已被 Codex Desktop 初始化

### Claude Code 不显示工具

检查：
- 二进制文件是否在 `PATH` 中
- MCP 服务器配置是否指向正确的可执行文件
- 编辑设置后是否重启了 Claude Code
- `codex-browser-bridge -mode discover` 是否在终端中正常工作

### CDP 命令失败

某些浏览器操作需要桥接器在发送 CDP 命令之前先 attach 到标签页。如果标签页是在桥接器外部打开的，请先列出用户标签页，然后 claim 目标标签页。

## 开发

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
make test
make build
```

## 路线图

可能的下一步：

- 更丰富的错误信息（pipe / 扩展常见故障）
- 非 Windows 平台的回退或明确平台限制
- 更好的截图输出处理
- 类型化的工具结果 schema
- 敏感域名的可选白名单 / 确认层
- Claude Code、Cursor、Codex CLI 等 MCP 客户端的使用示例

## 许可证

MIT License

## 免责声明

本项目为独立第三方项目，与 OpenAI、Codex Desktop、Anthropic、Claude Code、Google 或 Chrome 无关联、无认可、无从属关系。

## 致谢

感谢 [LINUX DO](https://linux.do/) 社区的支持与反馈。
