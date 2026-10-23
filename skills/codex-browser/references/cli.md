# Shell CLI

The optional shell entrypoint reuses the MCP registry, profiles, schemas,
handlers, safety checks, and output shaping. It does not add browser methods or
replace MCP session semantics.

```bash
codex-browser-bridge tools
codex-browser-bridge tools codex_locator
codex-browser-bridge call codex_list_browsers '{}'
codex-browser-bridge --max-output-bytes 16384 batch --finalize < calls.jsonl
```

`tools` lists names without connecting; `tools NAME` returns one schema. `call`
is one process and one fresh browser session. `batch` reads one
`{"name":"codex_...","arguments":{}}` object per line, keeps one session,
and stops on the first error. Use it for dependent calls after parsing each
result. Independent processes never share tabs or session IDs.

`--max-output-bytes` defaults to 16384 and accepts 256 through 8388608. It is a
CLI JSON-result limit separate from MCP text/image limits. Images are summarized
instead of emitted as base64. `--finalize` attempts `codex_finalize` after both
success and failure; this sends official `turnEnded`, so mark deliverables or
handoff tabs before ending the turn. A disconnected transport is not equivalent
to successful finalization.
