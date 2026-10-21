# DOM, Input, And Capture

Use this reference for page inspection, element discovery, forms, screenshots,
PDF rendering, device emulation, and performance checks.

## DOM And Accessibility

| Tool | Purpose |
|------|---------|
| `codex_dom_snapshot` | Full accessibility tree with clickable node IDs |
| `codex_dom_get_visible` | Compact human-readable visible DOM |
| `codex_dom_click` | Click by accessibility node ID |
| `codex_find_element` | Find elements by ARIA role and/or accessible name |
| `codex_click_element` | Click a node ID returned by `codex_find_element` |

Preferred element interaction:

```text
codex_find_element tab_id=<id> role=button name=Login
codex_click_element tab_id=<id> node_id=<id>
```

If the element is hard to locate by role/name:

```text
codex_dom_snapshot tab_id=<id>
codex_dom_click tab_id=<id> node_id=<id>
```

## Page Inspection

| Tool | Purpose |
|------|---------|
| `codex_get_url` | Read the current URL |
| `codex_get_title` | Read `document.title` |
| `codex_evaluate` | Evaluate JavaScript and return bounded JSON |
| `codex_page_assets` | List page resources and optionally fetch bounded content |
| `codex_console_logs` | Capture console output for a duration window |
| `codex_emulate_device` | Apply or reset a mobile viewport |
| `codex_performance_metrics` | Read Chrome performance metrics |

`codex_evaluate` supports promises and surfaced JavaScript errors. Use it for
targeted extraction, not as the first choice for a task already covered by a
typed tool.

## Capture And Page Control

| Tool | Purpose |
|------|---------|
| `codex_screenshot` | Capture the viewport as PNG, JPEG, or WebP |
| `codex_screenshot_element` | Capture one element through a clipped screenshot |
| `codex_print_pdf` | Render a bounded PDF and return a size summary |
| `codex_bring_to_front` | Activate a tab that is background-throttled |
| `codex_dialog` | Accept or dismiss alert, confirm, or prompt dialogs |

Screenshots on inactive tabs can time out. Call `codex_bring_to_front` first.
Prefer JPEG with an explicit quality when screenshot token size matters.

## Input

| Tool | Purpose |
|------|---------|
| `codex_click` | Click by CSS selector using JavaScript |
| `codex_fill` | Fill one CSS-selected input |
| `codex_hover` | Dispatch hover events |
| `codex_select_option` | Set a `<select>` and fire change/input events |
| `codex_drag` | Drag between coordinates |
| `codex_cua_click` | Click coordinates with real CDP mouse events |
| `codex_cua_type` | Type at current focus |
| `codex_cua_keypress` | Press a key sequence |
| `codex_cua_scroll` | Scroll at coordinates |
| `codex_click_and_wait` | Click and wait for load in one call |
| `codex_form_fill` | Fill multiple fields and optionally submit |
| `codex_file_input` | Upload files to an `<input type=file>` |

Use `codex_form_fill` for several fields. Use accessibility tools or
`codex_cua_click` when JavaScript `click()` is swallowed by overlays or custom
widgets. File upload requires `CODEX_BRIDGE_UPLOAD_BASE`; every path must be
absolute, inside that directory, and no larger than 10 MB.
