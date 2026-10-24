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
| `--session-file <path>` | one turn shared by several processes | off until you finalize |
| `--keep-turn` | one process, one turn | off |

- **Default cleanup preserves marks.** Unmarked agent tabs are closed and
  unmarked claimed user tabs are released; `deliverable` and `handoff` tabs
  survive, so mark anything you want to keep.
- **`--keep-turn` leaves an orphan.** The tab stays owned by a session that no
  longer exists, so it cannot be attached or closed from a later call. Use it
  only to inspect state by hand; prefer `batch` or `--session-file` for real
  multi-step work.
- **`--session-file` is a continuation token, not browser state.** It stores the
  session and turn identifiers so a later process resumes the same turn. It does
  not store cookies, storage, or tab content, and it is not portable between
  machines. `--finalize`, or a successful `codex_finalize` call, deletes it.
- **Interrupts clean up.** Ctrl-C and SIGTERM attempt the same turn cleanup once,
  with a bounded wait, before exiting.

`--finalize` can still be passed explicitly; it is already the default for a
one-shot command and only changes behaviour together with `--keep-turn` or a
session file.

## Multi-step work

```bash
# One process, several dependent calls. Stops at the first error.
codex-browser-bridge batch <<'JSONL'
{"name":"codex_create_tab","arguments":{}}
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
- Cleanup runs even when the command failed, so a partial workflow does not
  leave tabs behind.
- A disconnected transport is not equivalent to a successful finalization; the
  `codex_finalize` result says so explicitly.
