# DOM, Input, And Capture

Use this reference for page inspection, element discovery, forms, screenshots,
PDF rendering, device emulation, and performance checks.

## DOM And Accessibility

| Tool | Purpose |
|------|---------|
| `codex_dom_snapshot` | Full accessibility tree; use `backendDOMNodeId` for actions, not AX `nodeId` |
| `codex_dom_get_visible` | Compact human-readable visible DOM |
| `codex_dom_click` | Click by accessibility node ID |
| `codex_find_element` | Find by ARIA role/name; actionable backend `node_id` plus separate `ax_node_id` |
| `codex_click_element` | Click a node ID returned by `codex_find_element` |
| `codex_locator` | Strict selector/role/text/label/placeholder/test-id reads and actions |

Preferred element interaction:

```text
codex_find_element tab_id=<id> role=button name=Login
codex_click_element tab_id=<id> node_id=<id>
```

`actionable=false` means the AX entry has no backend DOM node and cannot be
passed to a DOM action. Refresh lookup after navigation or page replacement.
`codex_locator` can count, read text/attributes/value/state, click/double-click,
focus, press, fill, select, set checked state, or wait. It rejects ambiguous
matches by default; use `nth` only after inspecting matches. Its iframe/chained
locator semantics remain partial; it is not a Playwright API clone.

If the element is hard to locate by role/name:

```text
codex_dom_snapshot tab_id=<id>
codex_dom_click tab_id=<id> node_id=<backendDOMNodeId>
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

`codex_evaluate` supports promises and surfaced JavaScript errors. It is an
unrestricted page evaluator with a bridge-specific contract, not the official
read-only Playwright evaluate or locator-scoped sandbox. Use it for targeted
extraction, not as the first choice for a task already covered by a typed tool.

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
| `codex_click` | Resolve the first CSS match and click with a trusted CDP pointer |
| `codex_fill` | Fill one CSS-selected input |
| `codex_hover` | Resolve a CSS match and move a trusted CDP pointer over it |
| `codex_select_option` | Select an exact existing option through the locator engine |
| `codex_drag` | Drag through the supplied CUA path; legacy from/to becomes two points |
| `codex_cua_click` | CUA coordinate click with button 1..5, modifiers, and double-click count |
| `codex_cua_move` | Move the pointer with optional held modifiers |
| `codex_cua_type` | Type at current focus |
| `codex_cua_keypress` | Press a key sequence |
| `codex_cua_scroll` | Scroll at coordinates |
| `codex_click_and_wait` | Click and wait for load in one call |
| `codex_form_fill` | Fill multiple fields and optionally submit |
| `codex_file_input` | Upload files to an `<input type=file>` |

Use `codex_form_fill` for several fields. Prefer `codex_locator` with strict
matching; both locator clicks and `codex_click` use a trusted CDP pointer after
hit testing. File upload requires `CODEX_BRIDGE_UPLOAD_BASE`; every path must be
absolute, inside that directory, and no larger than 10 MB.
