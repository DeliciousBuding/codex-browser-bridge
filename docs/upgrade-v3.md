# Upgrading from 2.x to 3.0

`3.0.0` makes the bridge **native-only**: it relays the installed Codex
`node_repl` (Browser Use) and `cua_repl` (Computer Use) runtimes directly. The
separate Chromium extension adapter and its structured `codex_*` tools were
removed rather than kept as a second surface.

## What changed

1. **Claude Code plugin (`mcp.json`) and client examples (`examples/*.json`)**
   - `codex-browser` runs `--surface native-browser --mode mcp` (relays installed `node_repl`).
   - `codex-computer` runs `--surface computer --mode mcp` (relays installed `cua_repl`).
   - Each installed runtime advertises its own native tool catalog (`js`, `js_reset`, etc.). There is no bridge-side profile.
   - If only one native runtime is installed on your machine, the other plugin server failing at startup is expected; in manual MCP configs, register only the entry you use (`examples/native-browser.json` or `examples/native-computer-use.json`).

2. **Prerequisites for native entries**
   - Install and configure `[mcp_servers.node_repl]` and/or `[mcp_servers.cua_repl]` in Codex (`~/.codex/config.toml` or `--native-config <file>`).
   - For `--surface native-browser`, the extension backend also relies on the host environment variables (`BROWSER_USE_ACCESS_*`) provided by the Codex desktop host when configured via `env_vars`. Check setup with:
     ```bash
     codex-browser-bridge --surface native-browser --mode doctor
     codex-browser-bridge --surface computer --mode doctor
     ```
   - See [docs/native-computer-use.md](native-computer-use.md) for discovery, environment policy, and elicitation approvals.

3. **Removed 2.x surface**
   - `--surface browser`, `--mode cli`, `--pipe`, `--browser-family`, `--browser-instance`, `--profile`, `--upload-base`, `--max-text-bytes`, `--max-image-bytes`, `--max-output-bytes`, `--network-policy`, `--finalize`, `--session-file`, and `--keep-turn` no longer exist.
   - The structured `codex_*` tools, their `basic`/`network`/`full` profiles, the `codex://tabs` resource, the `login`/`extract-table` prompts, extension pipe discovery, and the extension-backed upload/download helpers are gone.
   - Bare CLI invocations without `--surface` now default to `--surface native-browser`.

4. **Rust library**
   - `src/` now contains only `main.rs`, `lib.rs`, `native.rs`, `cli.rs`, and `logging.rs`.
   - Library imports of the old browser/client/registry/discovery/locator/observation modules fail; migrate to the installed runtime's JavaScript API instead of re-adding a bridge-side implementation.

5. **Retired tooling and docs**
   - `docs/official-parity.md`, the drift baseline, `scripts/check-agent-surface.js`, `scripts/live-e2e.ps1`, the protocol benchmark, and the extension adapter tests were removed with the adapter.
   - The bundled skill routes to `native-computer-use.md`.

If you depend on the 2.x `codex_*` tools, stay on a 2.x release; 3.0 does not
provide a compatibility path.
