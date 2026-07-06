# Module Inventory

| Module | Responsibility | Dependencies | Files | Complexity | S.U.P.E.R Score |
|:--|:--|:--|--:|:--|:--|
| CLI/config | Parse flags, load config, choose mode | clap, config, client | `src/main.rs`, `src/cli.rs`, `src/config.rs` | Medium | S Green, U Green, P Yellow, E Green, R Green |
| MCP server | JSON-RPC stdio, tools/resources/prompts, handler dispatch | browser, client, schema | `src/mcp/*` | Medium | S Yellow, U Green, P Green, E Green, R Yellow |
| Browser operations | Implements 52 tool behaviors over CDP | client, security | `src/browser.rs` | High | S Yellow, U Green, P Yellow, E Yellow, R Yellow |
| Client transport | Pipe connection, pending map, reconnect, event subscriptions | protocol, pipe, discovery | `src/client.rs` | High | S Yellow, U Green, P Yellow, E Green, R Yellow |
| Protocol | Encode/decode length-prefixed frames | serde_json, tokio IO | `src/protocol.rs` | Low | S Green, U Green, P Green, E Green, R Green |
| Security | URL and upload file validation | std fs/path | `src/security.rs` | Low | S Green, U Green, P Yellow, E Yellow, R Green |
| Discovery/pipe/doctor | Find and probe Codex browser pipes | PowerShell, Tokio named pipe | `src/discovery.rs`, `src/pipe.rs`, `src/doctor.rs` | Medium | S Green, U Green, P Yellow, E Yellow, R Green |
| npm package | Wrapper, postinstall download, package metadata | Node built-ins, GitHub Release assets | `npm/*` | Medium | S Green, U Green, P Yellow, E Yellow, R Green |
| CI/CD | Test, supply-chain, release, npm publish | GitHub Actions, npm, gh | `.github/*` | Medium | S Green, U Green, P Yellow, E Green, R Green |
| Docs/skills/examples | Public usage and agent guidance | none | `README*`, `examples/*`, `skills/*` | Medium | S Yellow, U Green, P Yellow, E Yellow, R Yellow |

## Module Details

### MCP server
- **Path**: `src/mcp/`
- **Responsibility**: Parse MCP JSON-RPC, expose schemas, dispatch calls to browser operations, provide resources and prompts.
- **Public API**: `Server::run_stdio`, `Server::handle_jsonrpc_line`, `registered_tools`.
- **Notes**: Tool schemas are the public contract for agents. They must not claim unsupported input values such as `tab_id: "active"`.
- **S.U.P.E.R**: single-purpose split is mostly good; handler file remains broad because it mirrors the tool surface.

### Browser operations
- **Path**: `src/browser.rs`
- **Responsibility**: Convert tool-level intent into CDP or extension RPC calls.
- **Public API**: `navigate`, `wait_for_load`, `network_monitor`, `execute_cdp_generic`, and other tool helpers.
- **Notes**: The module is intentionally broad but contains testable pure helpers. Security-sensitive behavior should remain centralized and named.
- **S.U.P.E.R**: biggest pressure point for S and P; new work should extract pure helpers instead of adding ad hoc parsing inline.

### Client transport
- **Path**: `src/client.rs`
- **Responsibility**: Manage pipe IO, request IDs, pending responses, reconnect, attach caching, tab locks, and CDP events.
- **Notes**: Stability and resource-use improvements belong here, but changes need focused tests because this is the runtime core.
- **S.U.P.E.R**: replaceability is partial because `Client` is the concrete port; previous trait removal was intentional, so avoid reintroducing unused abstraction.

### CI/CD and npm
- **Path**: `.github/`, `npm/`
- **Responsibility**: Validate builds, create GitHub Releases, publish npm package, download release binaries.
- **Notes**: Current release flow has strong gates but had a package skill-bundling order bug and Dependabot/MSRV mismatch.
- **S.U.P.E.R**: environment assumptions should be explicit, especially Windows-only npm install and MSRV.
