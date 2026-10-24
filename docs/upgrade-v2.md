# Upgrade from 1.x to 2.0

Version 2.0 keeps the 66 tool names and existing browser-extension transport.
It changes tab claims, CLI continuation and configuration errors to preserve
browser identity. It attaches the same browser profiles; it does not move or
copy cookies, extensions or browser settings.

## Finish an existing CLI turn

A v1 `--session-file` stores session/turn IDs but no browser identity. Version
2.0 rejects it because choosing another connected profile would be unsafe.

Keep the original file and use the previous bridge version with the original
browser instance to finish that turn before creating a v2 continuation:

```sh
npx -y @delicious233/codex-browser-bridge@1.13.0 \
  --browser-instance ORIGINAL_INSTANCE_ID \
  --session-file /absolute/path/old-session.json call codex_finalize
```

Only use this command for a valid v1 file and a verified original instance.
If the original browser is unavailable or the file is damaged, preserve the
file and inspect/recover the original turn explicitly. Do not change the JSON
version number, guess the instance or discard the file as an automatic reset.
MCP stdio sessions do not use this file; finish their work before restarting
the client to load the upgraded plugin.

## Pass the observed tab snapshot

Call `codex_user_tabs`, then pass all three fields from the same returned entry:

```json
{
  "tab_id": "123",
  "expected_title": "Example Domain",
  "expected_url": "https://example.com/"
}
```

Use the exact title and URL, including empty strings or fragments. A changed
snapshot fails before claim. The native backend does not support atomic
snapshot comparison; a change during claim can leave ownership uncertain.
Inspect `codex_list_tabs` and `codex_user_tabs` before proceeding, not a blind
retry. The interactive REPL uses its most recent `user-tabs` snapshot.

## Validate configuration

An absent default `.codex-browser-bridge.toml` is optional. Explicit missing or
unreadable files, malformed TOML, unknown keys, empty selectors, invalid tool
profiles and response limits outside 1024..=8388608 bytes fail before browser
work. Remove unsupported keys and correct invalid values instead of relying on
ignored defaults. `--mode doctor` reports startup selector/source and transport
mode; `codex_get_info` reports the verified current binding.

Family and instance form one selector group. CLI > environment > file > default
chooses the highest-priority group specifying either field. For example, a CLI
family override no longer inherits a file's unrelated instance ID. Supply both
fields together when both constraints are intended.

## Install and verify

```sh
npm install -g @delicious233/codex-browser-bridge@2.0.0
codex-browser-bridge --version
```

For the Claude plugin:

```sh
claude plugin marketplace update codex-browser-bridge
claude plugin update codex-browser@codex-browser-bridge
claude plugin details codex-browser@codex-browser-bridge
claude mcp list
```

Start a new client session after updating; an already-running process retains
its old binary and schemas. Verify version 2.0.0 and connectivity, then select
the intended exact instance. Two profiles sharing a browser family are not
interchangeable, and separate owned tabs do not isolate one profile's storage.
