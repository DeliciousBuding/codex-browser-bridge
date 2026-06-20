---
name: codex-browser
description: 通过 codex-browser-bridge MCP server 操控 Codex Desktop 的 Chrome 浏览器。36 个工具覆盖标签管理、导航、DOM/页面检查、输入交互、CDP 原始命令、网络/Cookie、文件上传、弹窗处理、组合操作、诊断自检。
---

# Codex Browser Bridge — Agent 操作手册

你是通过 `codex-browser` MCP server 操控浏览器的 agent。本 skill 教你高效使用 36 个工具。

## 前置条件

- MCP server `codex-browser` 已连接（工具名以 `codex_` 开头）
- 使用前可先调用 `codex_doctor` 确认环境正常

## 工具分组速查

### [Tabs] 标签管理（5 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_list_tabs` | 列出当前 session 拥有的标签 | 无 |
| `codex_create_tab` | 创建空白标签 | 无（需随后 navigate） |
| `codex_close_tab` | 关闭标签 | `tab_id` |
| `codex_user_tabs` | 列出浏览器所有标签（包括未 claim 的） | 无 |
| `codex_claim_tab` | 认领一个用户标签到当前 session | `tab_id`（来自 user_tabs） |

### [Navigation] 导航（6 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_navigate` | 导航到 URL | `tab_id`, `url` |
| `codex_reload` | 刷新页面 | `tab_id` |
| `codex_navigate_back` | 后退 | `tab_id` |
| `codex_navigate_forward` | 前进 | `tab_id` |
| `codex_wait_for_load` | 等待 readyState=complete | `tab_id`, `timeout_ms`（默认 10000） |
| `codex_nav_and_wait` | 导航 + 等待加载（推荐） | `tab_id`, `url`, `timeout_ms` |

### [DOM] DOM/无障碍树（4 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_dom_snapshot` | 获取完整无障碍树（含 nodeId） | `tab_id` |
| `codex_dom_get_visible` | 人类可读的可见 DOM 树 | `tab_id` |
| `codex_dom_click` | 通过无障碍 nodeId 点击 | `tab_id`, `node_id` |
| `codex_find_element` | 按 ARIA role + name 查找元素 | `tab_id`, `role`?, `name`?, `max_results` |
| `codex_click_element` | 点击 find_element 返回的元素 | `tab_id`, `node_id` |

### [Page] 页面检查（3 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_screenshot` | 截取视口 PNG | `tab_id`, `full_page`（保留） |
| `codex_evaluate` | 执行任意 JS 并返回结果 | `tab_id`, `expression` |
| `codex_page_assets` | 列出页面资源（图片/CSS/JS/字体） | `tab_id`, `include_content`?, `types`? |

### [Input] 输入交互（11 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_click` | CSS 选择器点击（JS click()） | `tab_id`, `selector` |
| `codex_fill` | CSS 选择器填充输入框 | `tab_id`, `selector`, `value` |
| `codex_cua_click` | 精确坐标点击（CDP 鼠标事件） | `tab_id`, `x`, `y` |
| `codex_cua_type` | 在当前焦点输入文字 | `tab_id`, `text` |
| `codex_cua_keypress` | 按键序列（Enter/Ctrl+C 等） | `tab_id`, `keys: ["Enter"]` |
| `codex_cua_scroll` | 在坐标处滚动 | `tab_id`, `x`, `y`, `scroll_x`, `scroll_y` |
| `codex_click_and_wait` | 点击 + 等待加载 | `tab_id`, `selector`, `timeout_ms` |
| `codex_form_fill` | 批量填表 `{selector: value}` | `tab_id`, `fields`, `submit`? |
| `codex_file_input` | 上传文件到 `<input type=file>` | `tab_id`, `selector`, `files: ["C:/abs/path"]` |
| `codex_dialog` | 处理 alert/confirm/prompt | `tab_id`, `action: "accept"/"dismiss"`, `prompt_text`? |

### [Network] 网络/Cookie（2 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_network_cookies` | 读取 Cookie（默认脱敏） | `tab_id`, `urls`?, `redact_values`? |
| `codex_network_set_cookie` | 设置 Cookie | `tab_id`, `name`, `value`, `url`?, `domain`? |

### [CDP] 原始 CDP（1 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_execute_cdp` | 执行任意 CDP 命令（allowlist 保护） | `tab_id`, `method`, `params`? |

### [Session] 会话管理（3 个）
| 工具 | 用途 | 关键参数 |
|------|------|---------|
| `codex_name_session` | 命名当前 session | `name` |
| `codex_finalize` | 结束 session、清理所有标签 | 无 |
| `codex_get_info` | 获取扩展后端元数据 | 无 |
| `codex_doctor` | 自检：pipe 连通性、延迟、版本 | 无 |

## 常用工作流

### 1. 打开页面并截图

```
codex_create_tab → 记录返回的 tab_id
codex_nav_and_wait <tab_id> <url>
codex_screenshot <tab_id>
```

### 2. 认领已有标签并读取内容

```
codex_user_tabs → 找到目标标签的 tab_id
codex_claim_tab <tab_id>
codex_dom_get_visible <tab_id>  （或 codex_dom_snapshot）
```

### 3. 填写并提交表单

```
codex_nav_and_wait <tab_id> <url>
codex_form_fill <tab_id> {"#name": "Alice", "#email": "alice@example.com"} submit="#submit"
```

### 4. 通过无障碍树精准点击

```
codex_find_element <tab_id> role="button" name="登录"
codex_click_element <tab_id> <node_id>
```

### 5. 上传文件

```
codex_find_element <tab_id> role="button" name="上传"
codex_file_input <tab_id> "#file-input" files=["C:/Users/me/doc.pdf"]
```

### 6. 处理弹窗

```
codex_dialog <tab_id> action="accept" prompt_text="hello"
```

### 7. 环境自检

```
codex_doctor  → 确认 pipe 连通、Chrome 版本、延迟
```

## 工具选择原则

1. **导航优先用 `codex_nav_and_wait`**，不要分两步 navigate + wait_for_load
2. **填表优先用 `codex_form_fill`**，不要逐个 fill
3. **精准点击优先用 `codex_find_element` + `codex_click_element`**（基于无障碍树），而非 CSS 选择器猜测
4. **坐标点击用 `codex_cua_click`** 作为最后手段（CDP 鼠标事件，比 JS click() 更可靠）
5. **无法用专用工具时用 `codex_execute_cdp`**（CDP 逃生口）
6. **操作前用 `codex_doctor`** 确认环境
7. **结束后用 `codex_finalize`** 释放资源

## 安全注意

- `codex_file_input` 的文件路径必须是绝对路径，且在被允许的目录下（默认当前目录，可通过 `CODEX_BRIDGE_UPLOAD_BASE` 设置）
- Cookie 值默认脱敏（`[redacted]`），需显式传 `redact_values: false` 才能看到原始值
- URL 导航阻止 `file:` `javascript:` `data:` `vbscript:` 等危险 scheme
- CDP allowlist 阻止 Browser/Debugger/Target/Emulation/Security/Tracing 等危险域
- 截图和 DOM 内容可能包含敏感信息，避免公开发送

## 工具 Profile

通过 `CODEX_BRIDGE_PROFILE` 环境变量或 `--profile` CLI flag 控制工具暴露数量：

| Profile | 工具数 | 包含 |
|---------|:------:|------|
| `basic` | 25 | tabs + nav + dom + screenshot + 基础交互 |
| `network` | 32 | basic + cookies + CDP + file + dialog |
| `full` | 36 | 全部（默认） |

## 参考

- 项目 README: https://github.com/DeliciousBuding/codex-browser-bridge
- ROADMAP: https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ROADMAP.md
- 版本: v1.9.0（36 tools）
