# Changelog

All notable changes to this project will be documented in this file.

Format: one bullet per change, no hard line wraps. 中文版见 [CHANGELOG.zh-CN.md](CHANGELOG.zh-CN.md)。

## [Unreleased]

## [3.0.2] - 2026-11-19

### Fixed

- Confirm explicit `turn_ended` before clearing its execution context or rotating the default turn; retain rejected hooks for exit cleanup, preserve later executions, and report unanswered hooks without replaying them.
- Preserve native CLI JSON-RPC error frames, including `code`, `data`, and extension fields, on stdout with a nonzero exit.
- Validate CLI/doctor initialize responses against known legacy protocol versions and advertise form elicitation for explicit non-interactive cancellation.
- Align CUA skill entry selection and tab cleanup with the installed runtime: use known targets directly, leave released user tabs and existing persistent tabs open, and distinguish completion summaries from turn cleanup.

### Changed

- Document native string formatting and result preservation, CLI event/protocol boundaries, and host-owned tool exposure, token budgets, and UI rendering.

## [3.0.1] - 2026-11-19

### Breaking

- Removed `--surface browser`, the 66 structured `codex_*` tools, legacy CLI flags, and the extension adapter's modules, tests, and docs. Only `--surface native-browser` (default, `node_repl`) and `--surface computer` (`cua_repl`) remain. Users of the old tools must migrate or keep an older release. ([Upgrade Guide](docs/upgrade-v3.md))

### Added

- Added macOS arm64 and x64 prebuilt binaries, installer support, checksums, and attestations; macOS GUI/TCC acceptance remains unverified.
- Added Cursor plugin and MCP Registry manifests, OpenCode and VS Code templates, Antigravity configuration, and skills.sh installation guidance.
- Added CLI `--omit-inline-images` with `--image-dir` to save screenshots and artifact metadata while omitting base64 image data from stdout.

### Changed

- Aligned the bundled skill, setup guide, npm package, and security guidance with the two native runtimes; tool schemas and browser behavior come from the installed runtime.
- Bounded relay frames and outbound queues, batch input, and catalog pagination; split native unit tests into `src/native/tests.rs`.
- Rendered release notes from both changelogs with a shared install block.

### Fixed

- Added an importable `file://` `browser_client_url` to discovery and preferred concrete installed plugin versions over `latest` aliases.
- Rotated generated turn IDs after an explicit `turn_ended` and aligned native browser selection and non-interactive CLI approval cancellation.
- Corrected Windows release checksum generation to include both staged executable assets.
- Removed direct Unix libc references left behind by the dependency cleanup.

## [3.0.0] - 2026-11-19

### Breaking

- Claude Code plugin, client examples, and npm setup now register native Browser Use (`codex-browser` / `node_repl`) and Computer Use (`codex-computer` / `cua_repl`) separately instead of the 66-tool `codex_*` catalog. Pass `--surface browser` explicitly if you still need `codex_*` tools. ([Upgrade Guide](https://github.com/DeliciousBuding/codex-browser-bridge/blob/v3.0.0/docs/upgrade-v3.md))

### Added

- Native Browser Use relay via `--surface native-browser` (`node_repl`), sharing the same launcher, MCP forwarding, and `discover`/`doctor` diagnostics as native Computer Use.

### Changed

- Trimmed the bundled skill to native-first routing (`SKILL.md`, `native-computer-use.md`, `official-parity.md`) and removed obsolete 2.x references, unused library wrappers, and the legacy `Makefile`.

### Fixed

- Redacted malformed `~/.codex/config.toml` parse errors to line numbers only so local paths or tokens are never printed to stderr.
- Hardened native MCP lifecycle: late `tools/list` filtering after cancellation, deduplicated `turn_ended` teardown, early CLI argument validation, disconnect interruption reporting, and locator checkbox timeout handling.

## [2.1.1] - 2026-11-18

### Changed

- Aligned the native Computer Use launcher with Codex CLI: it preserves the host surface and trusted-service config, honors `env_vars` and the local stdio environment policy, resolves Windows commands against the constructed child `PATH` with `PATHEXT`, and applies configured startup/tool timeouts and tool filters.
- Unified native MCP relay and CLI/doctor on one untyped JSON stdio transport without an external MCP SDK; CLI and doctor negotiate `2025-06-18` with 30/300s startup/tool deadlines, while the relay leaves handshake negotiation, pagination, elicitation waits, and deadlines to the receiving host.
- Followed bounded native `tools/list` pagination in CLI and doctor, preserved call correlation metadata for every native tool call, and rejected unused `--image-dir` combinations before launch.
- Computer Use `js` now uses every native surface enabled by the host launch row; browser-specific network, tab, and output policies no longer wrap that surface.
- Decoupled client-to-native and native-to-client writes in the `--mode mcp` relay with cancel-safe outbound queues so backpressure in one direction cannot block the other.
- Expanded browser official parity: `selected_only` for `codex_list_tabs`, `networkidle` waits in `codex_observe`/`codex_nav_and_wait`/`codex_click_and_wait`, `url_glob` and safe `url_regex` matchers for `codex_wait_for_url`, log `levels` filtering, same-origin `include_iframes` snapshots, and padded element capture.

### Fixed

- Applied the host `enabled_tools`/`disabled_tools` policy consistently to advertised catalogs, direct `tools/call` requests, and internal `turn_ended` cleanup in both relay and CLI modes.
- Closed child `stdin` before failing native CLI/doctor startup, preserved the underlying error chain when cleanup is unconfirmed, and failed the relay when the native child closed `stdout` unexpectedly.
- Re-verified tab attachment before modifier-key release in `input::keypress`, bounded `pointer::scroll` to a single deadline, and guarded `start_observation` rollback by connection epoch.

## [2.1.0] - 2026-11-18

### Added

- Experimental `--surface computer` access to the installed native `cua_repl` MCP service, with native schemas, application elicitation, images, metadata, cancellation, and owned worker cleanup.
- Native CLI `call`/`batch`, runtime discovery/doctor, and optional image artifacts through `--image-dir`; noninteractive CLI never approves apps.

### Fixed

- Matched the native Codex Browser client by enabling focus emulation after every debugger attach, so background clicks, typing, and screenshots no longer activate the selected tab or window.
- Removed the automatic `Page.bringToFront` input fallback; foreground activation stays an explicit user-requested tool, and failed focus setup detaches and reports an error before dispatching input.
- Bound successful attachment setup to the pipe generation so reconnecting cannot send the first input with a stale debugger or focus cache.
- Removed pending request registrations when an outer timeout cancels a call, and let attachment recovery finish before the bounded pointer release.
- Preserved native MCP extension fields and installed host service mappings, rejected invalid call metadata per request, and reported interrupted sessions with a nonzero exit status.
- Finished bounded attach/focus setup before starting the observation producer, allowed bounded detach rollback, and avoided input release after expired setup.
- Positioned the pointer before every native scroll gesture; missing mouse-move acknowledgments are errors, while background scroll timeouts remain unknown outcomes and are never retried.

### Changed

- Updated pinned CI actions from reviewed dependency PRs #52, #53, #54, and #55.
- Raised the Rust MSRV to 1.88 for native MCP support (`rmcp` 3.9.0).
- Published desktop acceptance separately from source builds: Linux ARM64 X11 passed the synthetic app workflow, Windows input requires an unlocked desktop, and macOS GUI/TCC and Wayland remain unverified.

## [2.0.3] - 2026-11-17

### Fixed

- Required complete discovery for automatic and family-only browser selection; when uniqueness cannot be proven, the bridge requires an exact `browser_id` because an unprobed endpoint can be another profile.
- Ignored disconnected transports and app-private IAB/MCP Apps replies without treating ordinary timeouts or permission failures as stale; doctor distinguishes these cases in endpoint status and counts.

## [2.0.2] - 2026-11-17

### Fixed

- Allowed `select_browser_from_inventory` to select a unique live extension while other pipes or sockets fail `getInfo`, which previously broke auto-discovery and `--browser-family` on Windows `codex-browser-use-<uuid>` pipes and stale Linux `.sock` files. The bridge now logs a warning when one live browser matches, still fails closed on multiple live matches, and still reports `incomplete` when none is found.

## [2.0.1] - 2026-11-16

### Fixed

- Attaching to ChatGPT browser extension 1.26+ no longer fails with `Cannot read properties of undefined (reading 'timeoutMs')`; the bridge sends the official `{constraints: []}` network policy, which installs no rules, on every attach.

### Added

- `--network-policy`, `CODEX_BRIDGE_NETWORK_POLICY`, and the `network_policy` TOML key set the Browser Use network policy installed with each attach, validated against the extension budgets (128 constraints, 4096 patterns, 64 KiB pattern text, 1–30000 ms timeout) before the debugger detaches.

### Changed

- Moved the pinned official plugin baseline to `26.1111.21308` (23 interfaces, 153 members); `check-official-drift.js` counts both command-registration packing styles.

## [2.0.0] - 2026-10-25

See [docs/upgrade-v2.md](docs/upgrade-v2.md) for existing claims, configuration, and continuation files. Tool names and counts are unchanged.

### Changed

- CLI continuation files use v2, bind exact extension identity, and reject legacy or malformed tokens; OS-backed locks reject concurrent use, state is saved before operations, and it survives uncertain disconnects.
- `codex_claim_tab` requires the expected title and URL from current user-tab enumeration, and numeric-only calls are rejected; because native claim lacks atomic snapshot comparison, a changed response reports a possible ownership change without replay.
- Explicit configuration errors and unknown keys now fail; browser selectors use coherent CLI/environment/file precedence, and diagnostics distinguish tool profiles, attached-instance identity, and permission-gated capabilities.

### Fixed

- Incomplete discovery can no longer prove an implicit unique browser; rebind/finalize exclude in-flight tools, unresolved old turns stay recoverable, and rebinding the same instance preserves its turn.
- Explicit native no-route responses count as stale endpoints rather than unknown browser instances; timeout and permission failures still block implicit selection.

## [1.13.0] - 2026-10-24

### Added

- `codex_observe`: bounded tab/document event journal with cursor gaps, frame/loader state, active dialog and chooser IDs, and persistent rendered logs; capture begins before the action.
- `codex_file_chooser`: explicit interception with opaque document-bound handles, multiple mode, and bounded colocated uploads.
- Locator and/or/has/has_not combinators, bounded specs, limited safe regex, exact select descriptors, trial/force, sequential typing, trusted `setChecked`, and same-target async postconditions without replay.
- Screenshot bounded clip/scale, network URL substring filter, and a local MCP guide resource.

### Fixed

- Pointer and keyboard input activate a background tab before dispatching, because Chrome acknowledges synthesized input for an occluded tab without delivering it; `CODEX_BRIDGE_NO_AUTO_FOREGROUND=1` restores the previous behaviour.
- A dropped extension debugger now clears the cached attachment, and a sticky-window timeout also invalidates the cache; short caller deadlines used by input cleanup deliberately do not.
- `codex_navigate`, `codex_nav_and_wait`, and `codex_wait_for_load` surface `net::ERR_*` navigation results and browser error pages instead of reporting success.
- Keyboard events and `Input.insertText` use the non-replayed CDP path, so attachment recovery cannot duplicate a keydown or inserted text.
- `codex_hover` confirms the target reaches the `:hover` state before reporting success.
- `codex_console_logs` and `codex_network_monitor` unwrap the extension's `onCDPEvent` envelope and scope captures to the requested tab before byte accounting.

### Changed

- Exports and page assets share bounded acquisition/output budgets with strict explicit directories, staging, and atomic no-overwrite publication; filesystems lacking the atomic contract fail without a destructive fallback.
- Navigation composites arm before the action and wait for commit, DOMContentLoaded, or load; navigation, dialog, and file mutations do not replay on ambiguous errors.
- `call` and `batch` end the browser turn on exit by default, including failures; `--keep-turn` opts out and `--session-file` keeps one turn across separate commands, with Ctrl-C and SIGTERM following the same policy.
- `--session-file` is rejected outside `call`/`batch`, `--finalize` conflicts with `--keep-turn`, and a command rejected before dialing no longer creates a phantom session file or deletes an existing one.
- Mock-transport client tests (read loop, reconnect, event routing) now run on Windows as well as Linux.
- `codex_browser_management` requires `confirm=true` for methods that remove or discard browser state (`remove`, `delete`, `clear`, `close`, `discard`).
- `CODEX_BRIDGE_SOCKET_DIR` is an explicit Unix override, and `$XDG_RUNTIME_DIR/codex-browser-use` is tried before `/tmp`; returned socket paths identify their directory.
- Configuration precedence now matches the documented order: CLI flags > environment > config file > default.

## [1.12.0] - 2026-10-24

### Added

- 64 MCP tools (`basic` 45 / `network` 63 / `full` 64): `codex_locator` adds strict CSS/role/text/label/placeholder/test-id reads and actions, `codex_cua_move` adds pointer movement, and coordinate or backend-node clicks support double-click.
- CUA pointer parameters follow the official surface (`button` 1..5, `keypress`/`keys` modifiers, `scrollX`/`scrollY`, drag `path[]`), served by one non-replayed CDP dispatcher that releases after partial press/move failures; CSS `codex_click` and `codex_hover` use the locator hit test and scroll uses `Input.synthesizeScrollGesture`.
- Shell `tools`, `call`, and same-session `batch` commands reuse MCP profiles and handlers, default to 16384 bytes per result, summarize images, and support `--finalize` cleanup even after failure.
- Linux `/tmp/codex-browser-use/*.sock` discovery with socket owner/mode, directory, and peer-UID checks; doctor reports transport kind and path while retaining legacy `pipe_*` fields.
- Native Linux GNU x64/arm64 CI and release jobs plus npm package selection (glibc >= 2.39 for release binaries); Windows asset names stay stable.
- Browser-instance discovery and exact binding: `codex_list_browsers` reports stable instance IDs, browser family, profile metadata, capabilities, and transport, while `codex_bind_browser` pins one instance with fail-closed multi-instance selection and instance-pinned reconnect.
- Official capability alignment for browser history, Chromium management, viewport override, visibility, tab handoff/deliverable marks, and full-page screenshots.
- `docs/official-parity.md` and `scripts/check-official-drift.js` track changes to the bundled Codex Browser plugin API, command catalog, package manifests, and extension-family catalog.

### Changed

- Documented bridge-owned surfaces separately from official APIs: CUA input follows official field names, AX and backend DOM IDs stay distinct, and clipboard, WebMCP, page assets, exports, and the evaluate escape hatch keep explicit bridge contracts.
- The `basic` and `network` profiles include discovery/binding, strict locators, history, management, viewport, visibility, handoff marks, clipboard, WebMCP, and content export.

### Fixed

- Interactive CLI `try` commands preserve raw JSON argument strings.
- Doctor output includes browser family, extension instance ID, extension ID, and profile name for every connected pipe.
- Browser rebinding closes the old transport and requires prior finalization or explicit `end_previous_turn=true` when tabs are owned.
- `codex_finalize` sends official `turnEnded` and preserves marked handoff/deliverable tabs; a disconnected pipe is not successful turn cleanup.
- Accessibility lookup returns backend DOM `node_id`, separate AX IDs, and an `actionable` flag; AX-only nodes are not passed to DOM actions.
- Connection verification and subsequent requests use the same session and turn IDs.
- Doctor separates stale transport-name notes from health warnings and explains ambiguous same-family instances.

### Security

- Updated `tokio` to `1.53.3` for `RUSTSEC-2026-0386`.

## [1.11.4] - 2026-10-22

### Changed

- The Claude plugin launches the npm package through a version-pinned `npx` entry, so the one-command marketplace install works without a prior global npm install; a global install remains a warm-path option for the manual MCP examples.
- README, architecture, contributor, release, and example docs describe the plugin/binary boundary consistently: the plugin ships the skill and MCP configuration, while the pinned npm package supplies the Windows binary on first use.
- The plugin-surface check enforces the `npx` package pin and the one-command install string in both READMEs.

## [1.11.3] - 2026-10-21

### Changed

- The bridge remains a generic MCP server with the complete 52-tool surface and dual-era MCP support; Claude Code remains the primary plugin distribution path with progressive skill loading and tool-search deferral.
- Installer hints direct Codex / ChatGPT desktop users to their native browser integration instead of installing a duplicate bridge plugin.

### Removed

- Removed the portable Agent Plugins / OpenAI manifest, `.codex-plugin/` compatibility manifest, repo-scoped Codex marketplace, and associated install instructions.

## [1.11.2] - 2026-10-21

### Added

- Installable plugin distribution for Claude Code, ChatGPT desktop, and Codex via a portable Agent Plugins manifest, Codex compatibility manifest, Claude marketplace, and Claude plugin manifest, retaining the complete 52-tool MCP server.
- Claude Code MCP tool-search integration: `codex_doctor` is the only `anthropic/alwaysLoad` tool and the remaining schemas stay deferred.
- The agent skill became a progressive bundle: `SKILL.md` handles routing and core workflow while four task references load only when relevant.

### Changed

- Aligned runtime diagnostics, package metadata, architecture, contributor, security, and agent-skill wording with the current ChatGPT desktop app and browser extension names.
- Installer and doctor-suggested MCP configs recommend the 34-tool `basic` profile; the built-in default stays `full`, and plugin installs default to `full` because tool-search-capable clients defer schemas.
- npm installation hints recommend the Claude Code plugin first, then the ChatGPT desktop / Codex marketplace, with skill copy and manual MCP JSON as fallbacks.
- Pinned GitHub Actions updates: `actions/attest` 4.2.2, `codecov/codecov-action` 7.1.1, and `taiki-e/install-action` 2.87.21.

### Fixed

- Plugin packaging reuses one root `mcp.json`, so Claude no longer auto-loads a second project-level copy and starts the MCP endpoint twice.
- The live E2E harness drains doctor stdout/stderr concurrently instead of waiting on a full pipe buffer.
- Windows npm command shims (`.cmd`, `.bat`, and the extensionless shim) count as spawn-ready even when they wrap a different install path.
- Doctor pipe diagnostics report current browser-extension metadata instead of leaving the browser label empty.

## [1.11.1] - 2026-08-19

### Added

- npm installer honors `HTTPS_PROXY` / `https_proxy` (with `ALL_PROXY` fallback and `NO_PROXY` exclusions) when downloading release binaries, using HTTP CONNECT tunneling, which fixes `npm i -g` postinstall failures on proxy-only machines.

### Notes

- No Rust code changes; the binary rebuild only carries the version bump.

## [1.11.0] - 2026-08-19

### Added

- MCP protocol revision `2026-07-28` support (dual-era server): the legacy era (`2024-11-05` … `2025-11-25`) keeps the classic stdio lifecycle with version negotiation and a byte-identical wire format to v1.10.x, while modern-era requests carrying `params._meta["io.modelcontextprotocol/protocolVersion"]` are answered statelessly with `resultType`, server identity in `_meta`, and `ttlMs`/`cacheScope` hints; unsupported versions return `-32022`.
- `server/discover` advertises supported versions, capabilities, and instructions and is answered without version metadata so modern clients can probe stdio compatibility; `ping` is answered for legacy clients only.
- MCP tool annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) on all 52 tools; `openWorldHint` is always true because every tool reaches a live browser.
- `initialize` results include natural-language `instructions` describing effective bridge usage.

### Changed

- New `src/mcp/lifecycle.rs` owns version negotiation, era detection, modern result framing, and spec error envelopes.
- Verified against the official `2026-07-28` schema and Codex v0.148.x extension capabilities (`viewport`, `pageAssets`, `browserTabMentions` protocol v1).

## [1.10.1] - 2026-07-14

### Fixed

- `codex_evaluate` sets `awaitPromise=true` on `Runtime.evaluate`, so async expressions such as `fetch(...).then(...)` return real values instead of an empty Promise object.
- `codex_evaluate` surfaces `exceptionDetails` as a tool error instead of returning a silent partial payload.

### Documentation

- Documented `codex_evaluate` Promise-await and exception behavior in README (EN/ZH) and the packaged agent skill.

## [1.10.0] - 2026-07-10

### Changed

- `codex_file_input` requires an explicit `CODEX_BRIDGE_UPLOAD_BASE` instead of falling back to the working directory.
- npm prereleases publish to the `next` dist-tag while stable releases stay on `latest`.
- Navigation accepts only `http://` and `https://`, normalizes URLs before CDP, and rejects raw whitespace, control characters, backslashes, or missing hosts.
- Added bounded duration validation for MCP waits, captures, and form-fill delays, and added `codex_doctor` to the `basic` profile.
- Restricted generic CDP execution to an explicit low-level method allowlist that blocks raw methods bypassing dedicated safety wrappers.

### Fixed

- Bounded MCP stdio line reads and validated cookie-tool URL filters with the same `http`/`https` boundary used by navigation.
- Skipped page asset content fetches when advertised sizes exceed the byte budget, added fetch timeouts, and normalized fetched content or unknown sizes.
- Staged GitHub Release assets through a recoverable draft/update path and published the GitHub Release before npm so installs can download public binaries immediately.
- Validated release jobs against the exact annotated tag commit SHA and required non-empty dated changelog sections.
- Pinned release, CI, and supply-chain workflow actions to full commit SHAs and added a CI-enforced pin scanner plus an agent-surface drift check.
- Made release validation fail early on empty changelog sections and let npm reruns skip published versions only when the expected dist-tag is correct.
- Added workflow concurrency controls to cancel stale PR runs and serialize same-tag releases.
- Retired per-tab client locks on close, finalize, and attach failure, and normalized invalid page asset sizes so long sessions do not accumulate stale or misleading state.
- Streamed `codex_print_pdf` through bounded CDP with explicit stream close instead of holding unbounded base64 in memory.
- Drained E2E stderr asynchronously and covered the timeout path with a CI fake-bridge harness.
- Bundled multi-client MCP config examples in the npm package, trimmed published scripts to `postinstall` only, verified the bundled skill before publish, and included README and LICENSE in the tarball.
- Made explicit config paths authoritative, rejected relative upload paths, cleaned pending CDP requests on asset timeouts, and normalized fetched asset content to base64.
- Updated `crossbeam-epoch` to clear `RUSTSEC-2026-0204` from the dev-dependency benchmark chain, and aligned Dependabot with the Rust 1.85 MSRV by holding back `criterion` minor updates.

### Documentation

- Documented release/tag/changelog/npm provenance policy, multi-client configuration examples, upload-base setup, profile behavior, and numeric tab ID schemas.

## [1.9.1] - 2026-06-21

First release since v1.7.0. Tool count goes from 28 to 52, and agents can now watch what a page does, not just drive it.

### Added

- Event subscription layer routes server-pushed CDP events to subscribers: `codex_network_monitor` captures request↔response pairs as `[{url, method, status, mime_type}]`, and `codex_console_logs` captures `console.*` output.
- `codex_bring_to_front` activates a background tab via `Page.bringToFront`, and sticky CDP calls use a 20s budget instead of sharing the 60s total so throttled tabs fail fast.
- `codex_wait_for_element`, `codex_wait_for_url`, `codex_hover`, `codex_select_option`, `codex_drag`, `codex_storage`, `codex_get_url`, `codex_get_title`, `codex_screenshot_element`, `codex_print_pdf`, `codex_performance_metrics`, `codex_delete_cookies`, and `codex_emulate_device`.
- `codex_screenshot` accepts `format` (png/jpeg/webp) and `quality`; JPEG output is much smaller than PNG.
- `codex-browser-bridge --mode doctor` prints pipe health, Chrome version, and latency as JSON without an agent.

### Fixed

- Event subscription previously sent only `params` to subscribers, so `network_monitor` could not tell event types apart; subscribers now receive the whole frame.

### Architecture

- The read loop routes frames carrying a `method` and no `id` to subscribers instead of dropping them, exposed as `subscribe_events`/`unsubscribe_events`; network monitoring pairs request and response events by `requestId` into a clean list.
- Silence past 20s on a sticky attach now means the tab is background-throttled, so the call falls through to a full re-attach instead of waiting the full 60s.

### Profiles

`basic` 33 / `network` 50 / `full` 52.

## [1.9.0] - 2026-06-20

### Added — 8 new MCP tools (28 → 36)

- `codex_file_input`: upload files to `<input type=file>` via `DOM.setFileInputFiles`, with `canonicalize` plus prefix-check path traversal defense, 10 MB limit, regular files only, and `CODEX_BRIDGE_UPLOAD_BASE` configuration.
- `codex_dialog`: handle JavaScript alert/confirm/prompt dialogs via `Page.handleJavaScriptDialog`, with accept/dismiss and optional prompt text.
- `codex_find_element`: find elements by ARIA role and/or accessible name in the AX tree and return node IDs for `codex_click_element`.
- `codex_click_element`: click by accessibility node ID via `DOM.resolveNode` → `DOM.getBoxModel` → input dispatch, with no JS injection.
- `codex_nav_and_wait` and `codex_click_and_wait`: composite navigate/click plus wait-for-load calls.
- `codex_form_fill`: fill multiple form fields from a `{selector: value}` map, optionally submitting.
- `codex_doctor`: self-diagnostics that enumerate pipes, probe connectivity, and report latencies and browser versions.

### Changed

- `mcp.rs` split into `src/mcp/{mod,types,schema,handlers}.rs`, each under 400 lines; centralized `src/security.rs` provides `validate_url` and `validate_file_path`.
- Tool profiles: `basic` (25) / `network` (32) / `full` (36) via `CODEX_BRIDGE_PROFILE` or `--profile`; `validate_url` moved to `security.rs`, `Tool` visibility relaxed to `pub(crate)`, and `Server::new_with_profile()` added.

### Documentation

- Updated ROADMAP.md with v1.9.0 completion status and SUPER scores.

## [1.8.0] - 2026-06-20

### Added

- CDP error normalization: CDP-level errors (Target closed, and so on) surface as `isError: true` through a new `BridgeError::Cdp` variant carrying the method name and code.
- Sticky attach: per-tab CDP session caching skips redundant detach+attach round-trips, cutting RTT by roughly 50% for repeated calls on the same tab.
- `codex_network_cookies` redacts cookie values by default (`redact_values: true`), and `codex_network_set_cookie` enforces URL validation.
- `codex_execute_cdp` uses a method allowlist that blocks the Browser/Debugger/Target/Emulation/Security/Tracing domains, and `codex_page_assets` exposes the extension `pageAssets` capability with frameId-aware fetching.

### Changed

- All 28 tools gained group tags (`[Tabs]`, `[Navigation]`, `[DOM]`, `[Page]`, `[Input]`, `[CDP]`, `[Network]`, `[Session]`), cross-references, and clearer LLM guidance.
- `fullPage` became `full_page` (backward-compatible fallback), `timeout_ms` became an integer schema type, and the CLI moved from `browser.rs` to `src/cli.rs`.

### Removed

- Removed the `BridgeClient` trait as over-engineering (no mock consumer existed), the Go legacy tree (`internal/`, `cmd/`, `go.mod`, `go.sum`, `.golangci.yml`, −5382 lines), and `is_debugger_error` in favor of `is_session_invalid_error` with eight patterns.

### Performance

- `encode_frame` merges the length header and payload into one `write_all`; MCP stdio uses `BufReader::read_until` with a reusable buffer, and sticky attach avoids detach+attach on cached sessions.

### Security

- CDP error messages are stripped of `\n` and `\r` before surfacing, the session cache is cleared on `finalize` and healed on errors, and cookie value redaction is on by default.

### Documentation and Governance

- Added 15 GitHub topics, enabled Discussions, set the homepage, added npm keywords/`homepage`/`bugs`, and shipped `examples/` templates for claude-code, openclaw, hermes-agent, and cursor.
- Added `cargo clippy -D warnings` to CI and release, Rust coverage via `cargo-llvm-cov`, and `.gitignore` coverage for `docs/ask-gpt/`.
- Synchronized README.zh-CN, rewrote CONTRIBUTING.md for the Rust-only workflow, fixed stale SECURITY.md paths, and updated ROADMAP.md with v1.7.0–v2.0.0 plans.
- External reviews: a SUPER multi-dimension review scored CDP error normalization and sticky attach PASS_WITH_FIXES with all five must-fix items resolved, and a 3-agent architecture review plus an independent ChatGPT audit confirmed the architecture direction and priorities.

## [1.7.0] - 2026-06-20

### Added

- `codex_execute_cdp`: generic CDP executor for any method and params, the universal escape hatch for all CDP domains.
- `codex_page_assets`: lists page resources via `Page.getResourceTree` with optional base64 content fetch via `Page.getResourceContent`.
- `codex_network_cookies` (URL-filtered `Network.getCookies`) and `codex_network_set_cookie` (standard cookie attributes).

### Changed

- MCP tool count grew from 24 to 28, and browser helpers now include `execute_cdp_generic`, `get_resource_tree`, `get_resource_content`, `get_cookies`, and `set_cookie`.

### Tests

- Added `tests/cdp_tools_e2e.rs` with mock pipe infrastructure, 7 e2e tests, and 4 schema validation tests; total tests went from 85 to 92.

### Governance

- AGENTS.md documents MCP tool design patterns and security rules, and a public repo security audit found no secrets, tokens, or sensitive data.

## [1.6.0] - 2026-06-09

### Added

- Rebuilt the bridge binary in Rust with parity coverage for the extension wire protocol, pipe discovery, MCP tools, CLI commands, and browser helpers.
- CI gained a Windows Rust rewrite job for `cargo check`, `cargo test`, and x64/arm64 release-mode builds.
- Release validation checks `Cargo.toml` against the release tag and extracts notes from the tagged changelog section.
- Rust rewrite docs describe the npm compatibility plan and local build commands.

## [1.5.4] - 2026-06-09

### Fixed

- `codex_close_tab` retires the tab CDP lock when Chrome reports that the target is already gone.

## [1.5.3] - 2026-06-09

### Fixed

- Release workflow publishes npm after GitHub Release assets are created, manual runs check out the requested tag before test, lint, build, and publish jobs, and npm publish verifies `npm/package.json` against the tag.
- npm installer prefers package-embedded binary checksums, with the release checksum file kept as a fallback for older packages and development overrides.
- CUA click, type, and keypress keep each high-level input sequence under one tab lock and retry debugger detach errors inside the sequence; `codex_wait_for_load` bounds each CDP request by the caller's timeout.
- CDP tab locks are retired after successful tab close and session finalization.
- README, SECURITY, and issue templates name the local browser data that must be redacted before sharing output.

## [1.5.2] - 2026-06-09

### Fixed

- Release tags run test and lint jobs before GitHub Release assets are created, and CI runs Node tests for installer checksum parsing.
- Public issue and contributing docs ask reporters to redact browser-local data, and ignore rules cover generated coverage files, npm pack output, and package archives.
- The npm installer downloads assets for the package version by default, and development download overrides require `CODEX_BRIDGE_ALLOW_DEV_DOWNLOADS=1`.
- README wording states project relationships and browser-session permissions directly.

## [1.5.1] - 2026-06-09

### Fixed

- Serialized per-tab CDP detach, attach, and execute sequences so concurrent actions on one tab no longer detach each other.
- `codex_click` errors when the selector is missing, `codex_dom_click` rejects short `DOM.getBoxModel` arrays instead of panicking, and `codex_wait_for_load` retries transient navigation-time CDP errors until the timeout.
- Pipe discovery ignores the bare `codex-browser-use` namespace entry, and MCP JSON-RPC handling ignores notifications, rejects malformed envelopes and zero-length frames, and validates required tool arguments before opening pipe calls.
- The npm package uses a committed JavaScript command wrapper, downloads assets for the package version instead of `latest`, rejects unsupported CPU architectures, and checks package contents in CI.
- Manual release workflow runs require a valid `v*` release tag.

## [1.5.0] - 2026-06-05

### Fixed

- Pipe discovery works after Codex Desktop 26.602+: newer versions create pipe names with backslash separators (`codex-browser-use\<uuid>`), which PowerShell's `Get-ChildItem` treated as directories, so discovery now uses `[System.IO.Directory]::GetFileSystemEntries` with manual prefix stripping.
- `extractUUID` strips both `-` and `\` separators after the prefix, and the pipe warning threshold moved from >1 to >2 because old and new formats can coexist during upgrades.

## [0.3.0] - 2026-05-19

### Fixed

- Critical: raised the MCP read buffer from the 4 KB default to 10 MB, stopped answering `notifications/initialized` with an error, fixed CLI whitespace-only input panic and EOF busy-loop, and made fill report element-not-found instead of silent success.
- High: all 19 MCP tool handlers check `json.Unmarshal` errors, Click/Fill JavaScript literals use `json.Marshal` instead of `%q`, `Response.ID` became `*int` so `id:0` is not misclassified as a notification, and `writeResult`/`writeError` marshal errors are checked.
- High: history navigation validates both array bounds, PowerShell pipe discovery has a 15s timeout, and `readLoop` no longer blocks on duplicate response sends.
- Medium: `CUAType` dispatches keyDown+char+keyUp per CDP spec after a single debugger attach, health checks use a 5s timeout, CLI `try` extracts JSON from `args[2:]`, DOM snapshot fallback is marked, and box-model access checks array length.
- Low: `newUUID` returns an error with a `math/rand` fallback instead of panicking, `BRIDGE_DEBUG_LOG` open failures log a warning, `os.Exit` moved to `main()` so deferred cleanup runs, `extractUUID` strips one separator conditionally, timers are stopped on defer, and claim auto-attach errors are logged.
- Low: added `SendNotification` coverage, replaced a fragile slice with `strings.HasPrefix` in the wait-for-load timeout test, and made the E2E screenshot test validate non-empty base64.

### Security

- URL validation blocks `file:`, `javascript:`, `data:`, and `vbscript:` schemes, multiple `codex-browser-use-*` pipes raise a spoofing warning, error messages are stripped of newlines before logging, the JSON escape fallback returns `""` instead of propagating invalid JSON into evaluated scripts, and a missing `ping` handler was added.
- Test mocks now enforce exact method matches and detect duplicate responses.

### Changed

- `codex_dom_get_visible` is documented as returning a human-readable tree without node IDs (`codex_dom_snapshot` supplies node IDs), and `codex_screenshot` documents `fullPage` as reserved and viewport-only.

## [0.2.0] - 2026-05-16

### Added

- `codex_navigate_back` and `codex_navigate_forward` MCP tools expose the existing client history navigation.
- `codex_wait_for_load` polls `document.readyState` until `complete` or timeout, and `codex_screenshot` returns MCP `image` content so agents can view screenshots directly.
- `MCPServer.SetVersion` flows the build version into the MCP `initialize` handshake, and `NewMCPServerWithIO` allows testable I/O injection.
- Unit tests across `protocol`, `client`, `discovery`, and `mcp` with an in-memory `net.Pipe` fake server, a concurrent `SendRequest` stress test under `-race`, wire-format invariants, and full client-to-MCP handler tests; CI runs `go test -race -cover`.

### Fixed

- `discovery.extractUUID` no longer truncates UUIDs containing hyphens, pipe-not-found and dial errors are clearer, `Makefile install-local` copies the `.exe` on Windows, and duplicate README install options were removed.

### Internal

- Added `client.NewFromConn` for wrapping an existing `net.Conn`, and ignored `cover.out` and `*.coverprofile`.

## [0.1.0] - 2026-05-16

### Added

- Named pipe discovery for `codex-browser-use-*` pipes with go-winio connections.
- Session management (`createTab`, `getTabs`, `getUserTabs`, `claimUserTab`, `closeTab`) and CDP navigation (`Page.navigate`, `Page.reload`, `Page.getNavigationHistory`).
- CDP screenshot (base64 PNG), accessibility DOM snapshot, JavaScript evaluation, selector click/fill, and CUA input via `Input.dispatchMouseEvent`/`Input.dispatchKeyEvent`.
- MCP server (stdio JSON-RPC) with 20 tools, an interactive CLI debug mode, and a discover mode for listing active pipes.

### Notes

- Wire protocol uses camelCase method names (`getInfo`, not `get_info`), `executeCdp` requires the nested `{target: {tabId}}` format, `attach` must precede any CDP command, and each pipe connection creates a new browser session.
