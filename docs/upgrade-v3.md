# Upgrading from 2.x to 3.0

`3.0.0` switches the Claude Code plugin, client templates in `examples/`, and npm setup hint to **native-first**: the bridge relays the installed Codex `node_repl` (Browser Use) and `cua_repl` (Computer Use) runtimes directly instead of registering the 66 structured `codex_*` tools by default.

## What changed

1. **Claude Code plugin (`mcp.json`) and client examples (`examples/*.json`)**
   - `codex-browser` now runs `--surface native-browser --mode mcp` (relays installed `node_repl`).
   - `codex-computer` runs `--surface computer --mode mcp` (relays installed `cua_repl`).
   - `CODEX_BRIDGE_PROFILE` is no longer used by the plugin or native examples; each installed runtime advertises its own native tool catalog (`js`, `js_reset`, etc.).
   - If only one native runtime is installed on your machine, the other plugin server failing at startup is expected; in manual MCP configs, register only the entry you use (`examples/native-browser.json` or `examples/native-computer-use.json`).

2. **Prerequisites for native entries**
   - Install and configure `[mcp_servers.node_repl]` and/or `[mcp_servers.cua_repl]` in Codex (`~/.codex/config.toml` or `--native-config <file>`).
   - For `--surface native-browser`, the extension backend also relies on the host environment variables (`BROWSER_USE_ACCESS_*`) provided by the Codex desktop host when configured via `env_vars`. Check setup with:
     ```bash
     codex-browser-bridge --surface native-browser --mode doctor
     codex-browser-bridge --surface computer --mode doctor
     ```
   - See [docs/native-computer-use.md](native-computer-use.md) for discovery, environment policy, and elicitation approvals.

3. **Keeping existing 2.x `codex_*` workflows (`--surface browser`)**
   - The structured Chromium extension adapter (`codex_*`, 66 tools, `basic`/`network`/`full` profiles) remains available for compatibility.
   - Pass `--surface browser --mode mcp` explicitly in manual MCP configs or scripts:
     ```json
     {
       "mcpServers": {
         "codex-browser-legacy": {
           "command": "codex-browser-bridge",
           "args": ["--surface", "browser", "--mode", "mcp"],
           "env": { "CODEX_BRIDGE_PROFILE": "basic" }
         }
       }
     }
     ```
   - Bare CLI invocations without `--surface` continue to default to `--surface browser` so existing shell scripts do not silently switch surfaces.

4. **Rust library removals**
   - Removed four unused wrapper functions from `codex_browser_bridge::browser` (`dom_cua_type`, `get_resource_content`, `nav_and_wait`, `click_and_wait`); use `cua_type`, `get_resource_content_with_timeout`, and `navigation_action_and_wait`.
   - Removed the orphaned 2.x skill reference files from `skills/codex-browser/references/`; the bundled skill now routes to `native-computer-use.md` and `official-parity.md`.
