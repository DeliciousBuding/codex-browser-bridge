# Upgrading to 3.0.x

`3.0.2` fixes turn-cleanup acknowledgement, preserves structured CLI RPC errors,
and aligns the CUA skill with native entry selection and tab retention. CLI
RPC failures now print the original error response frame on stdout and exit
nonzero; consumers should distinguish its `error` field from native tool result
objects. CLI/doctor reject unsupported initialize protocol versions; MCP relay
negotiation remains the receiving host's responsibility. See
[native setup](native-computer-use.md#cli) for the supported versions and
[lifecycle](native-computer-use.md#lifecycle) for unconfirmed-hook behavior.

`3.0.0` made the installed Codex `node_repl` (Browser Use) and `cua_repl`
(Computer Use) runtimes the default entries while retaining the explicit
`--surface browser` compatibility adapter. `3.0.1` removes that adapter and its
structured `codex_*` tools. The bridge now relays only the two native runtimes.

The compatibility removal is included in `3.0.1` despite the patch version.
If your configuration uses `--surface browser` or any of the removed flags
below, update it before upgrading from 2.x or 3.0.0.

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

3. **Compatibility surface removed in 3.0.1**
   - `--surface browser`, `--mode cli`, `--pipe`, `--browser-family`, `--browser-instance`, `--profile`, `--upload-base`, `--max-text-bytes`, `--max-image-bytes`, `--max-output-bytes`, `--network-policy`, `--finalize`, `--session-file`, and `--keep-turn` no longer exist.
   - The structured `codex_*` tools, their `basic`/`network`/`full` profiles, the `codex://tabs` resource, the `login`/`extract-table` prompts, extension pipe discovery, and the extension-backed upload/download helpers are gone.
   - Bare CLI invocations without `--surface` now default to `--surface native-browser`.

4. **Rust library**
   - Production code is in `main.rs`, `lib.rs`, `native/mod.rs`, `cli.rs`, and `logging.rs`; native unit tests are in `native/tests.rs`.
   - Library imports of the old browser/client/registry/discovery/locator/observation modules fail; migrate to the installed runtime's JavaScript API instead of re-adding a bridge-side implementation.

5. **Retired tooling and docs**
   - `docs/official-parity.md`, the drift baseline, `scripts/check-agent-surface.js`, `scripts/live-e2e.ps1`, the protocol benchmark, and the extension adapter tests were removed with the adapter.
   - The bundled skill routes to `native-computer-use.md`.

If you depend on `codex_*` tools, keep the version you already use until you
have migrated. `3.0.1` provides no compatibility surface.
