---
name: codex-browser
description: Control Chrome via Codex Desktop's browser bridge. 52 MCP tools for tabs, navigation, DOM, input, CDP, network, file upload, dialog handling, and diagnostics.
---

# Codex Browser Bridge

You are an agent controlling a real Chrome browser through the `codex-browser` MCP server. This skill covers all 52 tools and their effective use.

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

### Navigation (8 tools)

| Tool | What it does |
|------|-------------|
| `codex_navigate` | Go to URL. Blocks `file:`, `javascript:`, `data:` schemes |
| `codex_reload` | Reload current page |
| `codex_navigate_back` | Back |
| `codex_navigate_forward` | Forward |
| `codex_wait_for_load` | Poll `document.readyState` until complete or timeout (default 10s) |
| `codex_nav_and_wait` | Navigate and wait for load in one call. **Prefer this over navigate + wait_for_load.** |
| `codex_wait_for_element` | Poll a CSS selector until it matches. Use on SPAs where `wait_for_load` returns immediately but content renders async. |
| `codex_wait_for_url` | Poll until `location.href` contains a substring (SPA route change). |

### DOM & Accessibility (5 tools)

| Tool | What it does |
|------|-------------|
| `codex_dom_snapshot` | Full AX tree with node IDs for `codex_dom_click` |
| `codex_dom_get_visible` | Human-readable DOM (no node IDs) |
| `codex_dom_click` | Click by AX node ID from snapshot |
| `codex_find_element` | Search by ARIA `role` and/or `name`. Returns node IDs. |
| `codex_click_element` | Click a result from `codex_find_element`. Uses CDP mouse events — no JS injection. |

### Page inspection (7 tools)

| Tool | What it does |
|------|-------------|
| `codex_get_url` | Current URL via `location.href` |
| `codex_get_title` | Current `document.title` |
| `codex_evaluate` | Run arbitrary JS, get JSON result |
| `codex_page_assets` | List page resources (images, CSS, JS, fonts) |
| `codex_console_logs` | Capture `console.*` output for a duration window (frontend debugging) |
| `codex_emulate_device` | Override viewport to emulate a device (`reset=true` to clear) |
| `codex_performance_metrics` | Chrome Performance metrics — DOM nodes, JS heap, event listeners |

### Capture (5 tools)

| Tool | What it does |
|------|-------------|
| `codex_screenshot` | Viewport PNG. **Times out on background tabs** — call `codex_bring_to_front` first. |
| `codex_screenshot_element` | Capture a single element via clipped screenshot |
| `codex_print_pdf` | Render page to PDF via `Page.printToPDF` |
| `codex_bring_to_front` | Activate a background tab. Restores its rendering pipeline so screenshot/CDP calls respond again. |
| `codex_dialog` | Handle `alert`/`confirm`/`prompt`. Accept or dismiss. |

### Input (12 tools)

| Tool | What it does |
|------|-------------|
| `codex_click` | CSS selector click via JS `click()`. Prefer AX methods for complex UI. |
| `codex_fill` | Set input value and fire events, by CSS selector |
| `codex_hover` | Dispatch mouseover + mousemove (dropdowns, tooltips, hover cards) |
| `codex_select_option` | Set `<select>` value + fire change/input (plain fill won't) |
| `codex_drag` | CDP mouse drag from point to point (sliders, sortable lists) |
| `codex_cua_click` | Click at `(x, y)` via CDP mouse events. Most reliable for complex UI. |
| `codex_cua_type` | Type at current focus |
| `codex_cua_keypress` | Key sequence. E.g. `["Control", "c"]`, `["Enter"]` |
| `codex_cua_scroll` | Scroll at `(x, y)` by `(scroll_x, scroll_y)` delta |
| `codex_click_and_wait` | Click and wait for load in one call |
| `codex_form_fill` | Fill multiple fields: `{"#name": "Alice", "#email": "a@b.com"}`. Optional `submit` selector. |
| `codex_file_input` | Upload files to `<input type=file>`. Paths must be absolute. 10 MB per file. |

### Network & state (5 tools)

| Tool | What it does |
|------|-------------|
| `codex_network_cookies` | Read cookies. Values redacted by default — pass `redact_values: false` to see them. |
| `codex_network_set_cookie` | Set a cookie. URL is validated. |
| `codex_delete_cookies` | Delete cookies by name (logout / account switch) |
| `codex_storage` | Get/set `localStorage` (login state, tokens, SPA state) |
| `codex_network_monitor` | Pair request↔response into a structured list (url, method, status, mime) for a duration window |

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

### Login flow (SPA, elements render async)
```
codex_nav_and_wait <tab_id> <login_url>
codex_wait_for_element <tab_id> selector="#username"   // SPA renders async
codex_form_fill <tab_id> {"#username": "alice", "#password": "secret"}
codex_click_element... or codex_click_and_wait <tab_id> "#login-btn"
codex_wait_for_element <tab_id> selector=".dashboard"   // confirm login landed
```

### Extract tabular data
```
codex_evaluate <tab_id> "JSON.stringify([...document.querySelectorAll('table tr')].map(r=>[...r.querySelectorAll('td')].map(c=>c.innerText)))"
// returns rows as JSON — parse and use directly
```

### Debug a failing interaction
```
codex_console_logs <tab_id> duration_ms=5000   // capture while reproducing
codex_network_monitor <tab_id> duration_ms=5000 // see what requests fired
```

## Error Recovery

When a tool fails, the cause is usually one of these. Try the fix before retrying.

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `codex_screenshot` or any CDP call **times out** silently | Tab is background-throttled/discarded by Chrome | `codex_bring_to_front <tab_id>` then retry |
| **attach failed** or "target closed" | Tab was opened outside the bridge, or session dropped | `codex_user_tabs` → `codex_claim_tab <tab_id>`, retry |
| **Element not found** by selector | Page changed, or selector is wrong | `codex_dom_snapshot` / `codex_find_element` to re-locate by role+name |
| **Click has no effect** | JS `click()` swallowed by overlay or shadow DOM | Switch to `codex_cua_click` (real CDP mouse events) |
| **SPA never "loads"** | URL unchanged, `wait_for_load` returns instantly | Use `codex_wait_for_element` on the target element instead |
| **All tools slow / erratic** | Pipe degraded or extension stalled | `codex_doctor`; if unhealthy, restart Codex Desktop |

General: always `codex_finalize` when the browsing task is done to release tabs.

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
| `basic` | 33 | tabs, nav, dom, screenshot, bring_to_front, core interaction |
| `network` | 50 | basic + cookies, CDP, file upload, dialog |
| `full` | 52 | everything (default) |

## Security

- Cookie values are redacted by default. Ask before passing `redact_values: false`.
- File paths must be absolute and within the upload directory.
- Screenshots and DOM snapshots may contain sensitive page content.
- Dangerous URL schemes (`file:`, `javascript:`, `data:`) are blocked.
