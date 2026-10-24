# Shell CLI

The optional shell entrypoint reuses the MCP registry, profiles, schemas,
handlers, safety checks, and output shaping. It does not add browser methods or
replace MCP session semantics.

```bash
codex-browser-bridge tools
codex-browser-bridge tools codex_locator
codex-browser-bridge call codex_list_browsers '{}'
codex-browser-bridge --max-output-bytes 16384 batch < calls.jsonl
```

`tools` lists names without connecting; `tools NAME` returns one schema.

## Session model

The unit of browser ownership is a **turn**, and a turn belongs to the process
that opened it. A tab created by one process cannot be attached by another, so
the shell closes the turn it opened unless you explicitly keep it.

| Command | Sessions | Turn cleanup |
|---|---|---|
| `call NAME [JSON]` | one process, one turn | **on by default** (`codex_finalize` on exit) |
| `batch` | one process, one turn | **on by default**, including after a failed line |
| `--session-file <path>` | one turn continued by sequential processes, exact instance bound | off until `--finalize` or `codex_finalize` |
| `--keep-turn` | one process, one turn | off |

- **Default cleanup preserves marks.** Unmarked agent tabs are closed and
  unmarked claimed user tabs are released; `deliverable` and `handoff` tabs
  survive, so mark anything you want to keep.
- **`--keep-turn` leaves an orphan.** The tab stays owned by a session that no
  longer exists, so it cannot be attached or closed from a later call. Use it
  only to inspect state by hand; prefer `batch` or `--session-file` for real
  multi-step work.
- **`--session-file` is a continuation token, not browser state.** It stores the
  v2 session/turn identifiers and exact browser instance so a later process resumes
  the same turn. An incompatible selector is rejected before saved IDs are sent.
  Concurrent invocation fails under an OS file lock; its parent directory must
  exist. Legacy/corrupt files fail explicitly; preserve them and finalize/recover
  the original turn before starting a new continuation. The empty `.lock`
  sidecar stays to prevent inode races, but process exit releases the lock. It does
  not store cookies, storage, or tab content, and it is not portable between
  machines. `--finalize`, or a successful `codex_finalize` call, deletes it.
- **Interrupts follow the cleanup policy.** Ctrl-C and SIGTERM attempt turn
  cleanup only when the command is configured to finalize. `--keep-turn` and an
  unfinalized `--session-file` leave the turn open and keep the session file.

`--finalize` is the default for a one-shot `call`/`batch`. With a session file it
explicitly ends the shared turn and deletes the file. It conflicts with
`--keep-turn`; choose one lifecycle policy.

`--session-file` is accepted only for `call` and `batch`; MCP stdio and the
interactive CLI own their turn for the process lifetime and reject it.

## Multi-step work

```bash
# One process, several dependent calls on a tab id already returned by
# codex_user_tabs. Stops at the first error.
codex-browser-bridge batch <<'JSONL'
{"name":"codex_claim_tab","arguments":{"tab_id":"123","expected_title":"Example Domain","expected_url":"https://example.com/"}}
{"name":"codex_navigate","arguments":{"tab_id":"123","url":"https://example.com"}}
JSONL

# Several processes, one turn. Use the tab id printed by the first call.
codex-browser-bridge --session-file /tmp/bridge.json call codex_create_tab
codex-browser-bridge --session-file /tmp/bridge.json call codex_navigate '{"tab_id":"123","url":"https://example.com"}'
codex-browser-bridge --session-file /tmp/bridge.json call codex_finalize
```

`batch` reads one `{"name":"codex_...","arguments":{}}` object per line and
stops on the first error; later lines do not run. It has no variable
substitution, so a tab id must be known before the batch starts (for example
from `codex_user_tabs`).

## Output budget

`--max-output-bytes` defaults to 16384 and accepts 256 through 8388608. It is a
CLI JSON-result limit separate from the MCP text/image limits. Images are
summarized instead of emitted as base64, and oversized results are truncated
into a valid JSON envelope with `truncated` and `originalBytes`.

## Failure behaviour

- A failed tool call writes the JSON result to stdout and exits non-zero; the
  reason stays in the JSON, not only on stderr.
- Cleanup runs after a failed tool call when the command is configured to
  finalize, so a partial one-shot workflow does not leave tabs behind.
- A requested `--session-file` write or removal is part of the command result.
  If it fails, the process exits non-zero instead of claiming that a future
  process can resume the same turn.
- A disconnected transport is not equivalent to a successful finalization; the
  `codex_finalize` result says so explicitly.
