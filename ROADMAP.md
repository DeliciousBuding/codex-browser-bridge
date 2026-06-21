# ROADMAP

## Status: v1.9.1 shipped (2026-06-21)

52 MCP tools, CDP event architecture, structured network monitoring, `--mode doctor` CLI, JPEG/WebP screenshots. See [CHANGELOG.md](CHANGELOG.md) for the full v1.7.0 → v1.9.1 arc.

**Architecture health (SUPER):** S 5, U 5, P 5, E 3 (Windows-only), R 4 = **22/25**. The remaining gaps are operational maturity, not architecture — that is what v1.10.0 targets.

### Completed releases

- **v1.9.1** (2026-06-21): 16 new tools → 52 total. CDP event subscription (`network_monitor`, `console_logs`). Background-tab fix (`bring_to_front` + sticky 20s timeout). JPEG/WebP screenshots, sessionStorage, `--mode doctor`, `performance_metrics`.
- **v1.9.0** (2026-06-20): `src/mcp/` module split, 8 tools (file upload, dialog, locator layer, composite tools, profiles, doctor).
- **v1.8.0** (2026-06-20): CDP error normalization, sticky attach, Go legacy purged, protocol/MCP stdio optimization.
- **v1.7.x** (2026-06-20): CDP MCP tools, security hardening, Codecov, repo SEO.

---

## v1.10.0 — Engineering hardening (current)

The tool layer is saturated. The honest gaps are runtime robustness, supply chain, performance baselines, and test coverage — not more tools.

### P0 — Runtime robustness & supply chain

- [x] **Pipe auto-reconnect.** ✅ Done. Request-driven passive reconnect: read_loop exit marks `alive=false` + drains pending; the next `send_request` runs `ensure_alive` → `reconnect_locked` (discover + dial, 3-attempt backoff 0/100/250ms, 5s cooldown on full failure) → swaps writer + restarts read_loop + clears `attached_tabs`. Injectable `ConnectionFactory` (boxed-future, no async_trait) — real discovery in prod, `duplex()` mocks in tests. New `BridgeError::Connection` distinguishes dead-connection errors. 4 reconnect tests run under `cfg(not(windows))` via a new ubuntu `test-lib` CI job.
  - Effort: M · landed in `src/client.rs`, `src/error.rs`, `.github/workflows/ci.yml`
- [x] **Supply-chain CI** (`cargo-deny`). ✅ Done. `deny.toml` (advisories · licenses · bans · sources, tight allow list), `supply-chain.yml` (cargo-deny-action on push/PR + weekly), dependabot for `Cargo.toml`. cargo-deny's advisories check queries the same RustSec DB as cargo-audit, so it is a strict superset — no separate cargo-audit job.
  - Effort: S · landed in `deny.toml`, `.github/workflows/supply-chain.yml`, `.github/dependabot.yml`

### P1 — Performance & test baselines

- [x] **criterion benchmarks.** ✅ Done. `benches/protocol.rs` benchmarks frame encode/decode round-trip + `with_session_params` — the hot path every request traverses. `attach.rs` skipped: sticky-vs-full-attach is logic best covered by the reconnect tests, not a perf bench (would need a full CDP mock to be meaningful).
  - Effort: S · landed in `benches/protocol.rs`, `Cargo.toml` (criterion dev-dep)
- [ ] **Restore e2e coverage the right way.** The deleted `#[path]`-include tests were an anti-pattern (they embed the source tree and break under modularization — that is what killed the v1.8/v1.9 releases). Rewrite key flows (network_monitor event pairing, CDP error propagation, storage round-trip) as integration tests using the **public crate API** + tokio mock, not source-tree embedding.
  - Effort: M · files: `tests/`

### P2 — Distribution & protocol depth

- [ ] **winget + scoop manifests.** `winget install codex-browser-bridge` is more native than npm for Windows users. Discovery lift, no code.
  - Effort: S
- [ ] **MCP resources/prompts.** Expose cookies/localStorage as subscribable resources (agents read state without a tool call per access); ship prompt templates for login/table-extraction flows.
  - Effort: M
- [ ] **Config file** (`.codex-browser-bridge.toml`) for profile + upload_base, replacing env-only.
  - Effort: S
- [ ] **ARCHITECTURE.md** — design-decision record for contributors.
  - Effort: S

---

## v2.0.0 — Cross-platform

- [ ] Unix domain socket transport (macOS/Linux). Codex Desktop on Darwin uses a different IPC mechanism than Windows named pipes — needs research on the actual socket naming.
- [ ] Non-Windows CI matrix (ubuntu-latest, macos-latest).
- [ ] npm `os` field expanded to `darwin`, `linux`.
- [ ] WSL: auto-select the Windows named pipe from a WSL guest.
- Effort: L

---

## Backlog

Feature-sized items not yet scoped into a release:

- [ ] `codex_network_monitor` URL filter (reduce noise; the paired list is usable today)
- [ ] `codex_performance_trace` — `Performance.enable` + trace export (the metrics tool already shipped)
- [ ] `codex_execute_cdp` user-customizable allowlist (config/env)
- [ ] Typed tool result schemas (structured agent consumption)
- [ ] Screenshot clip/scale params (format + quality already shipped)
- [ ] OpenGraph social share image
- [ ] macOS socket-naming research + real Darwin test
