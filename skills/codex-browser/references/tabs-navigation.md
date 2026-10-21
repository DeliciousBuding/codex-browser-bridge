# Tabs And Navigation

Use this reference when opening, finding, claiming, navigating, or waiting on a
page.

## Tabs

| Tool | Purpose |
|------|---------|
| `codex_list_tabs` | List tabs owned by this bridge session |
| `codex_create_tab` | Create a blank tab |
| `codex_close_tab` | Close a bridge-owned tab |
| `codex_user_tabs` | List all browser tabs, including unclaimed tabs |
| `codex_claim_tab` | Take ownership of a tab returned by `codex_user_tabs` |

Open a new page:

```text
codex_create_tab
codex_nav_and_wait tab_id=<id> url=https://example.com
codex_screenshot tab_id=<id> format=jpeg
```

Reuse an existing logged-in page:

```text
codex_user_tabs
codex_claim_tab tab_id=<id>
codex_get_title tab_id=<id>
codex_dom_get_visible tab_id=<id>
```

`codex_close_tab` must only target a tab owned by the current bridge session.
Calling it on an unclaimed user tab should fail.

## Navigation

| Tool | Purpose |
|------|---------|
| `codex_navigate` | Navigate to an HTTP(S) URL |
| `codex_reload` | Reload the current page |
| `codex_navigate_back` | Go back in session history |
| `codex_navigate_forward` | Go forward in session history |
| `codex_wait_for_load` | Wait for `document.readyState` |
| `codex_nav_and_wait` | Navigate and wait in one call |
| `codex_wait_for_element` | Wait for an element in an SPA |
| `codex_wait_for_url` | Wait for an SPA route change |

Prefer `codex_nav_and_wait`; it saves a round trip. If an SPA reports loaded
before rendering content, follow with `codex_wait_for_element` or
`codex_wait_for_url` rather than repeating `codex_wait_for_load`.
