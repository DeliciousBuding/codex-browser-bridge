# Module Inventory: codex-browser-bridge

## Rust Modules (v1.6.0, `src/`)

### 1. `main.rs` — CLI Entry
- **Responsibility**: Mode dispatch (`mcp`/`discover`/`cli`), debug logging setup
- **Size**: ~80 lines
- **Dependencies**: `mcp`, `client`, `discovery`, `browser`
- **Complexity**: Low
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅

### 2. `mcp.rs` — MCP Server
- **Responsibility**: JSON-RPC 2.0 handling over stdio, tool registration, tool dispatch
- **Size**: ~605 lines
- **Dependencies**: `client`, `browser`, `serde_json`
- **Complexity**: Medium
- **24 registered tools** across tab management, navigation, inspection, interaction, session
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅
- **Key insight**: Adding new tools is a ~10-line change (tool def + handler + dispatch)

### 3. `browser.rs` — Browser Operations
- **Responsibility**: CDP wrappers, JS snippet builders, CLI interactive mode
- **Size**: ~844 lines
- **Dependencies**: `client`, `serde_json`, `tokio`
- **Complexity**: Medium-High (many CDP methods, JS injection)
- **CDP methods used**: Page.navigate, Page.captureScreenshot, Runtime.evaluate, Accessibility.getFullAXTree, DOM.getBoxModel, Input.dispatchMouseEvent, Input.insertText, Input.dispatchKeyEvent
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅

### 4. `client.rs` — Pipe Client
- **Responsibility**: Named pipe connection, JSON-RPC request/response, CDP execute wrapper, auto-discovery
- **Size**: ~307 lines
- **Dependencies**: `discovery`, `protocol`, `pipe`, `tokio`, `uuid`
- **Complexity**: Medium
- **Key**: `execute_cdp()` is generic — passes ANY CDP method through `executeCdp` RPC
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅

### 5. `discovery.rs` — Pipe Discovery
- **Responsibility**: PowerShell-based pipe enumeration, UUID extraction
- **Size**: ~50 lines
- **Dependencies**: `std::process::Command`, `regex`
- **Complexity**: Low
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅

### 6. `protocol.rs` — Wire Protocol
- **Responsibility**: Length-prefixed frame encode/decode (4-byte LE), Request/Response types
- **Size**: ~50 lines
- **Dependencies**: `serde`, `serde_json`
- **Complexity**: Low
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅

### 7. `pipe.rs` — Pipe Transport
- **Responsibility**: Windows named pipe dial
- **Size**: ~30 lines
- **Dependencies**: `tokio`, Windows API
- **Complexity**: Low
- **S.U.P.E.R Score**: S✅ U✅ P✅ E✅ R✅

### 8. `error.rs` — Error Types
- **Responsibility**: `BridgeError` enum (Protocol, Rpc, Discovery, User, Timeout, Io)
- **Size**: ~30 lines
- **Dependencies**: `thiserror`
- **Complexity**: Low

### 9. `lib.rs` — Library Root
- **Responsibility**: Module declarations
- **Size**: ~15 lines

### 10. `logging.rs` — Debug Logging
- **Responsibility**: `BRIDGE_DEBUG_LOG` env-var-controlled file logging
- **Size**: ~25 lines

## Go Modules (legacy v1.5.x, `internal/`)

| Module | Rust Equivalent | Status |
|--------|----------------|--------|
| `cmd/bridge/main.go` | `src/main.rs` | Migrated |
| `internal/client/` | `src/client.rs` | Migrated |
| `internal/mcp/server.go` | `src/mcp.rs` | Migrated |
| `internal/client/browser.go` | `src/browser.rs` | Migrated |
| `internal/discovery/` | `src/discovery.rs` | Migrated |
| `internal/protocol/` | `src/protocol.rs` | Migrated |

## MCP Tool ↔ CDP Method Mapping

| MCP Tool | Extension RPC | CDP Method(s) |
|----------|---------------|---------------|
| `codex_list_tabs` | `getTabs` | — |
| `codex_create_tab` | `createTab` | — |
| `codex_close_tab` | — | `Page.close` |
| `codex_user_tabs` | `getUserTabs` | — |
| `codex_claim_tab` | `claimUserTab` + `attach` | — |
| `codex_navigate` | — | `Page.navigate` |
| `codex_navigate_back` | — | `Page.getNavigationHistory` + `Page.navigateToHistoryEntry` |
| `codex_navigate_forward` | — | same as back, different index |
| `codex_reload` | — | `Page.reload` |
| `codex_wait_for_load` | — | `Runtime.evaluate` (poll `document.readyState`) |
| `codex_dom_snapshot` | — | `Accessibility.getFullAXTree` (fallback: `Runtime.evaluate`) |
| `codex_screenshot` | — | `Page.captureScreenshot` |
| `codex_click` | — | `Runtime.evaluate` (JS `querySelector` + `click()`) |
| `codex_fill` | — | `Runtime.evaluate` (JS `querySelector` + `.value =`) |
| `codex_evaluate` | — | `Runtime.evaluate` |
| `codex_cua_click` | — | `Input.dispatchMouseEvent` |
| `codex_cua_type` | — | `Input.insertText` |
| `codex_cua_keypress` | — | `Input.dispatchKeyEvent` |
| `codex_cua_scroll` | — | `Input.dispatchMouseEvent` (mouseWheel) |
| `codex_dom_get_visible` | — | `Runtime.evaluate` (JS DOM walker) |
| `codex_dom_click` | — | `DOM.resolveNode` + `DOM.getBoxModel` + `Input.dispatchMouseEvent` |
| `codex_name_session` | `nameSession` | — |
| `codex_finalize` | `finalizeTabs` | — |
| `codex_get_info` | `getInfo` | — |

## Codex Extension RPC Methods (complete)

| Method | Bridge Support | Notes |
|--------|---------------|-------|
| `getInfo` | ✅ `codex_get_info` | Returns capabilities, version, metadata |
| `getTabs` | ✅ `codex_list_tabs` | Session tabs |
| `createTab` | ✅ `codex_create_tab` | New tab |
| `getUserTabs` | ✅ `codex_user_tabs` | All browser tabs |
| `claimUserTab` | ✅ `codex_claim_tab` | Claim user tab |
| `attach` | ✅ (internal) | CDP debugger attach |
| `detach` | ✅ (internal) | CDP debugger detach |
| `executeCdp` | ✅ (internal) | Execute CDP command |
| `nameSession` | ✅ `codex_name_session` | Name session |
| `finalizeTabs` | ✅ `codex_finalize` | Clean up tabs |
| `ping` | ✅ (MCP level) | Health check |

## Extension Capabilities (from `getInfo`, v1.1.5)

```json
{
  "capabilities": {
    "tab": [{
      "id": "pageAssets",
      "description": "List assets already observed in the current page state
                       and bundle selected assets into a temporary local artifact."
    }]
  }
}
```

### pageAssets Analysis

- **Status**: Capability DECLARED but NO dedicated RPC method found
- **Likely mechanism**: Accessed through CDP `Page.getResourceTree` + `Page.getResourceContent`
- **Not yet exposed** as an MCP tool
- **Potential RPC methods probed** (all failed with "No handler registered"):
  - `getPageAssets`, `pageAssets`, `listPageAssets`, `getTabAssets`, `getAssets`
- **Conclusion**: This is a CDP-level capability, not an RPC method. The extension's `executeCdp` translates it.

## S.U.P.E.R Architecture Health Summary

| Principle | Score | Notes |
|-----------|-------|-------|
| **S**ingle Purpose | ✅ | Each module has one clear responsibility |
| **U**nidirectional Flow | ✅ | MCP → browser → client → pipe (clean layers) |
| **P**orts over Implementation | ✅ | `executeCdp` is the universal CDP port |
| **E**nvironment-Agnostic | ⚠️ | Windows-only (named pipes). No non-Windows fallback. |
| **R**eplaceable Parts | ✅ | Client, MCP server, browser helpers are independently testable |

### Violation Hotspots

1. **Windows-only constraint (E)**: Named pipe transport is inherently Windows-specific. Non-Windows support would require a different transport (TCP, Unix socket). Low priority for this task — user is on Windows.
2. **`pageAssets` gap**: Extension's declared capability has no bridge exposure. The `executeCdp` port can reach it but no ergonomic tool exists.
