# Official Browser Capability Parity

Use this reference when the task asks for a capability that is visible in the
Codex Browser plugin but not obvious in the older bridge tool names.

## Browser identity

- `codex_list_browsers` returns stable `browser_id`, `family`, `name`,
  `extension_id`, optional `profile_name`, capabilities, and pipe transport.
- `codex_bind_browser` accepts an exact `browser_id`. A family-only bind is
  valid only when one instance of that family is live.
- With owned tabs, rebinding requires prior finalization or explicit
  `end_previous_turn=true`; successful switching must not abandon an old turn.
- `codex_get_info` reports the currently bound instance.
- `codex_doctor` reports all probed pipe instances with identity fields.

Never treat `Chrome` and `Edge` as interchangeable when the user names one.
Never substitute another profile of the same family when the named
`extensionInstanceId` is unavailable.

`browser` and `chrome` are packaging names around the same Browser client,
service, and family catalog. `browserFamily` selects the extension/profile
backend; it does not select a different automation implementation.

## Newly aligned capabilities

- `codex_observe` supplies opt-in persistent logs/events, frames, loader epochs
  and dialog/chooser IDs; start before actions and preserve its cursor. Gaps
  and reconnect/stale invalidation are explicit, not full pre-start history.
- `codex_file_chooser` supports explicit interception and bounded colocated
  uploads. Remote browser paths/OOPIF/download completion remain unsupported.
- Locator and/or/has/has_not, limited safe regex, exact select descriptors,
  trusted checked postconditions and sequential typing share the same resolver.
  Trial skips input/DOM mutations, but may scroll; force cannot bypass invalid
  frame geometry. Full ARIA/cross-origin/Playwright semantics remain gaps.

- `codex_browser_history` maps `Browser.history` through `getUserHistory`.
- `codex_browser_management` maps the official `management` capability:
  `windows`, `tabs`, `tabGroups`, and `bookmarks`; methods that remove or
  discard state require `confirm=true` after the user explicitly requested it.
- `codex_browser_viewport` maps the official `viewport` capability.
- `codex_browser_visibility` maps the official `visibility` capability when the
  selected extension advertises it.
- `codex_mark_tab` maps `markHandoff` and `markDeliverable`.
- `codex_screenshot` supports the official full-page screenshot behavior.
- `codex_locator` covers strict selector/role/text/label/placeholder/test-id
  reads and actions. Clicks use a trusted pointer with left/middle/right and
  modifier names; iframe, force and full auto-wait semantics remain partial.
- `codex_cua_click`, `codex_cua_move`, `codex_cua_scroll` and `codex_drag` use
  the official CUA button/modifier/path fields through one non-replayed CDP
  pointer dispatcher; scroll uses `Input.synthesizeScrollGesture`. Current
  extensions do not acknowledge `mouseMoved`, so moves use a bounded no-ack
  window and drag time follows path length. Coordinate `downloadMedia` and
  human-like timing remain.
- `codex_click` and `codex_hover` are CSS convenience aliases over the same
  locator hit test and trusted pointer path, not JavaScript synthetic events.
- `codex_clipboard`, `codex_webmcp`, `codex_content_export` and
  `codex_page_assets` are bounded bridge-native implementations. They do not
  inherit app-side policy, artifact-library delivery or private command routing.
- `codex_finalize` sends official `turnEnded` and retains marks; closing the
  transport is not equivalent cleanup.

## Transport boundary

Not every name in the official `browser-service.mjs` command catalog is
available through the low-level extension pipe. The direct
`executeUnhandledCommand` path admits only a small capability subset; Playwright,
AX actions, dev logs, clipboard, WebMCP, and browserAuth are implemented above
that pipe. Use the bridge's documented CDP-backed tools instead of guessing and
forwarding an official command name. Optional shell commands reuse this same
surface and do not establish additional method-level parity.

## App-private capabilities

Secure credential handoff, Guardian/site approvals, enterprise policy,
ChatGPT artifact export, bot-detection reporting, Cloud Browser, and MCP Apps
are implemented above the extension pipe. The bridge cannot claim those
semantics by calling raw CDP. Apply the MCP client's confirmation policy and
report unavailable capabilities plainly.

The complete maintained matrix is `docs/official-parity.md` in the project
repository. After a Codex update, the maintainer runs
`node scripts/check-official-drift.js` before changing this skill.
