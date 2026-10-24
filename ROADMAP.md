# ROADMAP

## Current direction

Expose reliable Chromium-extension outcomes to non-Codex MCP clients, without
cloning app-private auth, Guardian, artifact libraries or policy. Tool counts
are distribution facts, not parity. Source contracts and remaining differences:
[official-parity.md](docs/official-parity.md). Release history:
[CHANGELOG.md](CHANGELOG.md).

## Implemented in current source

- Exact browser binding, no-replay input/navigation/dialog mutations, bounded
  turn/session cleanup and truthful failures.
- Opt-in tab/document observation with frame/loader state, dialog/chooser IDs,
  bounded persistent event/log cursors, explicit gaps and action-before-wait.
- Shared declarative locator composition, trusted checked-state transitions,
  exact option descriptors, bounded actionability/trial/force and typing.
- One controlled bridge-host file-result boundary with atomic no-overwrite
  publication; bounded colocated file chooser uploads, not remote-path guesses.
- Windows and Linux GNU x64/arm64 packaging; existing pinned plugin/npm workflow.

## Remaining supported-path gaps

- Cross-origin/OOPIF and transformed-frame input routing; complete ARIA name
  computation. Existing same-origin resolver is deliberately not Playwright.
- Backend-authorized browser download completion/path and remote upload transfer.
  Host/CDP cached resource export does not satisfy that contract.
- Full official navigation URL matchers/networkidle, level-filtered log APIs,
  unrestricted safe regex and persistent visible-DOM reference epochs.
- Optional typed result schemas and performance traces, only when explicitly
  justified by a client contract. Raw privileged CDP remains blocked.
- Expand synthetic live acceptance beyond current tested tool families; native
  compilation/CI does not establish runtime support for untested endpoints.

## Explicitly deferred

- macOS until a real Darwin endpoint is available and tested.
- winget/scoop: npm already delivers checksummed native binaries.
- App-private secure auth, cloud takeover, telemetry and artifact library.

WSL sockets and Windows interop pipes are distinct transports; no automatic
cross-OS discovery is claimed.
