---
name: codex-browser
description: Control Chrome via Codex Desktop's browser bridge. 37 MCP tools for tabs, navigation, DOM, input, CDP, network, file upload, dialog handling, and diagnostics.
---

# Codex Browser Bridge

You are an agent controlling a real Chrome browser through the `codex-browser` MCP server. This skill covers all 37 tools and their effective use.

## Quick Check

Call `codex_doctor` first. If it reports healthy, proceed. If not, tell the user to start Codex Desktop, Chrome, and the Codex extension.

## Tool Groups

### Tabs (5 tools)

| Tool | What it does |
|------|-------------|
| `codex_list_tabs` | Tabs owned by this session |
| `codex_create_tab` | New blank tab (navigate after) |
| `codex_close_tab` | Close a tab |
| `codex_user_tabs` | All browser tabs, including unclaimed |
| `codex_claim_tab` | Take ownership of an existing tab |

### Navigation (6 tools)

| Tool | What it does |
|------|-------------|
| `codex_navigate` | Go to URL. Blocks `file:`, `javascript:`, `data:` schemes |
| `codex_reload` | Reload current page |
| `codex_navigate_back` | Back |
| `codex_navigate_forward` | Forward |
| `codex_wait_for_load` | Poll `document.readyState` until complete or timeout (default 10s) |
| `codex_nav_and_wait` | Navigate and wait for load in one call. **Prefer this over navigate + wait_for_load.** |

### DOM & Accessibility (5 tools)

| Tool | What it does |
|------|-------------|
| `codex_dom_snapshot` | Full AX tree with node IDs for `codex_dom_click` |
| `codex_dom_get_visible` | Human-readable DOM (no node IDs) |
| `codex_dom_click` | Click by AX node ID from snapshot |
| `codex_find_element` | Search by ARIA `role` and/or `name`. Returns node IDs. |
| `codex_click_element` | Click a result from `codex_find_element`. Uses CDP mouse events — no JS injection. |

### Page (5 tools)

| Tool | What it does |
|------|-------------|
| `codex_screenshot` | Viewport PNG. Returns image content the model can see. **Times out on background tabs** — call `codex_bring_to_front` first. |
| `codex_evaluate` | Run arbitrary JS, get JSON result |
| `codex_page_assets` | List page resources (images, CSS, JS, fonts). Optional content fetch. |
| `codex_dialog` | Handle `alert`/`confirm`/`prompt`. Accept or dismiss. |
| `codex_bring_to_front` | Activate a background tab via `Page.bringToFront`. Restores its rendering pipeline so screenshot/CDP calls respond again. |

### Input (9 tools)

| Tool | What it does |
|------|-------------|
| `codex_click` | CSS selector click via JS `click()`. Prefer AX methods for complex UI. |
| `codex_fill` | Set input value and fire events, by CSS selector |
| `codex_cua_click` | Click at `(x, y)` via CDP mouse events. Most reliable for complex UI. |
| `codex_cua_type` | Type at current focus |
| `codex_cua_keypress` | Key sequence. E.g. `["Control", "c"]`, `["Enter"]` |
| `codex_cua_scroll` | Scroll at `(x, y)` by `(scroll_x, scroll_y)` delta |
| `codex_click_and_wait` | Click and wait for load in one call |
| `codex_form_fill` | Fill multiple fields: `{"#name": "Alice", "#email": "a@b.com"}`. Optional `submit` selector. |
| `codex_file_input` | Upload files to `<input type=file>`. Paths must be absolute. 10 MB per file. |

### Network (2 tools)

| Tool | What it does |
|------|-------------|
| `codex_network_cookies` | Read cookies. Values redacted by default — pass `redact_values: false` to see them. |
| `codex_network_set_cookie` | Set a cookie. URL is validated. |

### CDP (1 tool)

| Tool | What it does |
|------|-------------|
| `codex_execute_cdp` | Raw CDP command. `Browser`, `Debugger`, `Target`, `Emulation`, `Security`, `Tracing` domains blocked. |

### Session (4 tools)

| Tool | What it does |
|------|-------------|
| `codex_name_session` | Label the session |
| `codex_finalize` | Clean up all tabs. **Call when done.** |
| `codex_get_info` | Extension metadata |
| `codex_doctor` | Self-check: pipe health, Chrome version, latency |

## Workflows

### Open a page and see it
```
codex_create_tab
codex_nav_and_wait <tab_id> <url>
codex_bring_to_front <tab_id>     // if the tab may be backgrounded
codex_screenshot <tab_id>
```

### Claim an existing tab and read it
```
codex_user_tabs
codex_claim_tab <tab_id>
codex_dom_get_visible <tab_id>
```

### Find and click by ARIA role
```
codex_find_element <tab_id> role="button" name="Login"
codex_click_element <tab_id> <node_id>
```

### Fill and submit a form
```
codex_nav_and_wait <tab_id> <url>
codex_form_fill <tab_id> {"#name": "Alice", "#email": "a@b.com"} submit="#submit"
```

### Upload a file
```
codex_find_element <tab_id> name="Choose File"
codex_file_input <tab_id> "#file-input" files=["C:/Users/me/doc.pdf"]
```

### Handle a dialog
```
codex_dialog <tab_id> action="accept" prompt_text="hello"
// or dismiss:
codex_dialog <tab_id> action="dismiss"
```

## Rules of Thumb

1. **Prefer `codex_nav_and_wait`** — one round trip instead of two.
2. **Prefer `codex_form_fill`** — faster than multiple fill calls, less partial state.
3. **Prefer AX-based clicking** (`find_element` + `click_element`) over CSS selectors.
4. **Use `codex_cua_click`** when other click methods fail — raw CDP mouse events.
5. **Use `codex_execute_cdp`** only when no dedicated tool covers the operation.
6. **Call `codex_finalize`** when the browsing task is complete.
7. **Call `codex_doctor`** if tools return unexpected errors — the pipe may have disconnected.

## Profiles

If a tool you expect is missing, the server may be running a reduced profile. The user can configure this with `CODEX_BRIDGE_PROFILE` or `--profile`.

| Profile | Count | Scope |
|---------|:-----:|-------|
| `basic` | 26 | tabs, nav, dom, screenshot, bring_to_front, core interaction |
| `network` | 33 | basic + cookies, CDP, file upload, dialog |
| `full` | 37 | everything (default) |

## Security

- Cookie values are redacted by default. Ask before passing `redact_values: false`.
- File paths must be absolute and within the upload directory.
- Screenshots and DOM snapshots may contain sensitive page content.
- Dangerous URL schemes (`file:`, `javascript:`, `data:`) are blocked.
