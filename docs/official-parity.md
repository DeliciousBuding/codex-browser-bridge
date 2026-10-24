# Official browser capability parity

The target is the user-owned Chromium extension backend used by the official
Browser plugin, exposed to Claude Code and other MCP clients. Complete parity
requires matching inputs, results, lifecycle, and failure behavior. Tool counts
do not measure that coverage.

The pinned official plugin is `26.1016.61520`. Its `docs/api.json` contains
148 interface members: 129 callable member names and 19 properties. Overloads
share one member name. Optional capability methods appear in capability docs
and types, outside that 148-member count. The files and counts are pinned in
`official-surface-baseline.json`; `node scripts/check-official-drift.js` checks
the installed developer reference without redistributing it.

This inventory is a source/protocol audit. Synthetic locator tests run without
a browser. Source support, a mock wire test, and live extension acceptance are
separate evidence levels; an app-side command registry entry alone proves none
of the latter.

## Backend identity and transport

The installed `browser` and `chrome` packages share the Browser implementation,
API manifest, and browser-family catalog. Edge advertises the Chrome-compatible
backend protocol, but each profile remains a distinct browser instance.
`metadata.extensionInstanceId` is the managed identity. Names and pipe paths are
not substitutes. Extensions lacking that ID need an explicit legacy pipe override.

The official stack has three layers: the agent API, the app-side browser service,
and the extension's framed native transport. The bridge uses the native layer.
Existing native request methods include `getInfo`, `getTabs`, `getUserTabs`,
`getUserHistory`, `createTab`, `claimUserTab`, `markTab`, `nameSession`,
`turnEnded`, and `executeCdp`.

`executeUnhandledCommand` is not a general router for the app-side registry.
The existing direct path covers management and viewport; visibility depends on
the extension build. Playwright, AX actions, clipboard, logs, page-assets, and
WebMCP command names must not be forwarded speculatively. Their app-side logic
can often be implemented through CDP instead.

## Reading the mapping

- **Native**: an existing bridge native-transport path implements the outcome.
- **CDP partial**: useful behavior exists, with the differences listed below.
- **Implementable**: missing bridge behavior has a CDP/local implementation path.
- **Bridge-native**: implemented locally with a documented contract, without
  forwarding the app-side official adapter or claiming its private semantics.
- **Not extension baseline**: the manifest explicitly lists `extension` in
  `unsupportedByDefaultIn`; this is not an official-extension parity deficit.
- **App-private**: the exact behavior requires Codex services or UI, not merely
  page access. Local equivalents must have their own contracts.

None of these labels means every parameter or live browser build has passed.

## Core API members

| Interface / members | Mapping | Remaining behavior or boundary |
|---|---|---|
| `Agent.browsers`, `Agent.documentation` | Properties | Namespace references, not callable methods |
| `Browsers.list`, `get` | Native | Registry list and exact bind; bridge IDs are extension-instance IDs |
| `Browsers.getDefault`, `getForUrl` | Implementable policy | Hidden/undocumented helpers; choose only an unambiguous instance, never silently switch profiles |
| `Browser.browserId`, `capabilities`, `tabs`, `user` | Properties | Namespace/identity references; capability availability must distinguish extension advertisement from implemented bridge support |
| `Browser.documentation`, `Documentation.get` | Implementable local docs | MCP resources can expose bridge-specific guides without copying the official package |
| `Browser.history`, `nameSession` | Native | Preserve history ordering/date bounds and validate dates/nonempty queries, not only JSON types |
| `BrowserUser.openTabs`, `claimTab` | Native | Owned tabs and user tabs are distinct; preserve lastOpened/order/metadata when supplied |
| `BrowserUser.getTabContext` | Not extension baseline | Bounded read without claiming is excluded for extension; a separate implementation must preserve ownership and expected URL |
| `Tabs.get`, `list`, `new` | Native | Handle construction/get is a client abstraction; validate owned IDs against current binding |
| `Tabs.selected` | Implementable | Resolve the active owned tab, not any active user tab |
| `Tabs.content` | Not extension baseline | Temporary background URL extraction is useful but would be a bridge extension, with bounded load and cleanup |
| `Tab.ax`, `capabilities`, `clipboard`, `content`, `cua`, `dev`, `dom_cua`, `id`, `playwright` | Properties | Namespace references; `Tab.ax` itself is excluded from the extension default |
| `Tab.back`, `close`, `forward`, `goto`, `reload`, `title`, `url` | Native / CDP | Existing operations; navigation readiness/error semantics still differ from Playwright |
| `Tab.markDeliverable`, `markHandoff` | Native | `markTab`, followed by authoritative `turnEnded`; latest mark wins |
| `Tab.screenshot` | CDP partial | Viewport/full-page exist; official arbitrary `clip` still needs a bounded crop contract |
| `Tab.getJsDialog`; dialog `accept`/`dismiss` | CDP partial | Handling exists; querying active dialog, opaque dialog ID and stale-dialog rejection require buffered Page dialog events |
| `AlertDialog.type`, `BeforeUnloadDialog.type`, `ConfirmDialog.type`, `PromptDialog.type` | Properties | Discriminants, not functions |
| `Tab.requestManualHandoff`, `requestUserTakeover` | Not extension baseline | Cloud/manual-control UI is app-private; extension handoff uses a marked user-visible tab |

`codex_finalize` ends the current turn, preserves marked tabs, and rotates the
turn identity. It must not report remote cleanup as confirmed on a dead pipe.
Rebinding validates the replacement before ending the prior turn and refuses
to discard an owned live turn without the explicit cleanup option.

## Playwright and locator members

`codex_locator` is one result-oriented surface, not the official Playwright
engine. It resolves open shadow roots, optional same-origin `frame_selectors`,
descendant `scopes`, and the existing CSS/role/name/text/label/placeholder/test-ID
filters. `nth=-1` means last. Test IDs default to exact matching; text/name
matching remains normalized, case-insensitive substring matching unless exact.

| Interface / members | Mapping | Remaining behavior or boundary |
|---|---|---|
| `PlaywrightAPI.locator`, `getByRole`, `getByText`, `getByLabel`, `getByPlaceholder`, `getByTestId` | CDP partial | One declarative locator spec covers these outcomes; accessible-name and implicit-role algorithms are approximations, not the complete ARIA algorithms |
| `PlaywrightAPI.frameLocator`; all 7 `PlaywrightFrameLocator` members | CDP partial | Same-origin frame chains work; cross-origin/OOPIF routing, transformed-frame pointer geometry, and navigation epochs remain |
| `PlaywrightLocator.locator`, `getByRole`, `getByText`, `getByLabel`, `getByPlaceholder`, `getByTestId` | CDP partial | `scopes` provides descendant chains; locator objects and cross-frame chains are not API-compatible |
| `PlaywrightLocator.filter`, `first`, `last`, `nth` | CDP partial | `has_text`, `has_not_text`, `visible`, and nth exist; nested locator `has`/`hasNot`, regex matchers remain |
| `PlaywrightLocator.and`, `or`, `all` | Implementable | Declarative intersection/union and stable list-of-locator results need explicit serialization, not JavaScript handle strings |
| `PlaywrightLocator.count`, `getAttribute`, `isEnabled`, `isVisible` | CDP partial | Count/attribute/boolean reads exist; an absent target currently errors for single reads, while official `isVisible()` can return false |
| `PlaywrightLocator.innerText`, `textContent`, `allTextContents` | CDP partial | Rendered `inner_text`, raw `text_content` and `all_text_contents` are distinct; existing `text`/`all_text` are rendered-text aliases |
| `PlaywrightLocator.click`, `dblclick` | CDP partial | Scroll, viewport clipping, shadow/frame hit tests, left/middle/right button and Alt/Control/Meta/Shift modifiers use trusted CDP dispatch; full actionability auto-wait, force and trial remain |
| `PlaywrightLocator.fill` | CDP partial | Uses native input/textarea setters for controlled inputs, rejects hidden/disabled/readonly targets, emits input/change; not a complete browser editing algorithm |
| `PlaywrightLocator.check`, `uncheck`, `setChecked` | CDP partial | `set_checked` uses native DOM click and verifies state instead of assigning `checked`; it is not a trusted CDP pointer event, and framework transitions may be asynchronous |
| `PlaywrightLocator.selectOption` | CDP partial | Existing string value selection; descriptor `{value,label,index}`, multiple selection, missing-option failure and auto-wait remain |
| `PlaywrightLocator.press`, `type`, `pressSequentially` | CDP partial / implementable | Focus exists; correct keyboard chords, character events and append semantics belong in the shared input dispatcher |
| `PlaywrightLocator.waitFor` | CDP partial | Attached/detached/visible/hidden probes respect nth/strict; visibility approximations and fixed polling differ from official auto-wait |
| `PlaywrightLocator.evaluate`, `evaluateAll`; `PlaywrightAPI.evaluate` | Different contract | Current `codex_evaluate` runs unrestricted page JS and has no locator-scoped/read-only mode; it is not an official Playwright evaluate implementation |
| `PlaywrightAPI.domSnapshot` | Implementable | Existing raw AX tree and depth-limited visible DOM are not the official iframe-expanded DOM text snapshot |
| `PlaywrightAPI.elementInfo`, `elementScreenshot` | Implementable | CDP DOM hit testing, backend IDs, selector suggestions and annotated screenshot bounds |
| `PlaywrightAPI.expectNavigation`, `waitForLoadState`, `waitForURL` | CDP partial | Existing readyState/URL polling; event-before-action, URL matching, commit/domcontentloaded/load/networkidle and redirects need shared navigation tracking. `Page.navigate` transport success is not navigation success: `errorText` and `chrome-error://` pages are surfaced as failures |
| `PlaywrightAPI.waitForTimeout` | Client utility | Bounded delay, not browser capability coverage |
| `PlaywrightAPI.waitForEvent`; `PlaywrightDownload.path`; `PlaywrightFileChooser.isMultiple`, `setFiles` | Implementable, transport-dependent | File chooser events/interception and DOM file input are available CDP paths; download completion/path needs extension support or a verified browser-level download path |
| `PlaywrightLocator.downloadMedia` | Implementable, transport-dependent | Resolve a current observed media/link target, apply download permissions and return the browser-visible saved path; a generic host HTTP fetch is not equivalent |

The locator members share one resolver. Parameter coverage, frame scope,
actionability, timeout, and result semantics must be tested separately. A CSS
implementation of an action does not establish full locator parity.

## Input, accessibility, clipboard, and logs

| Interface / members | Mapping | Remaining behavior or boundary |
|---|---|---|
| `CUAAPI.click`, `double_click`, `move`, `scroll`, `type`, `keypress`, `drag` | CDP partial | One shared pointer dispatcher supports button 1..5, modifier names, exact drag paths and failure-safe release; mutations are not replayed. Synthesized input activates a hidden tab first, because Chrome acknowledges input for an occluded tab without delivering it | Scroll uses `Input.synthesizeScrollGesture`. Current extension builds do not acknowledge `mouseMoved`, so movement uses a bounded 3-second dispatch window. Human-like timing, hold duration and coordinate downloadMedia remain |
| `CUAAPI.downloadMedia` | Implementable, transport-dependent | Coordinate-to-media resolution and verified saved-file handling |
| `DomCUAAPI.click`, `double_click`, `keypress`, `type` | CDP partial | Backend-node actions exist; official visible-DOM local refs are not raw AX IDs |
| `DomCUAAPI.get_visible_dom`, `scroll`, `downloadMedia` | Implementable | Current visible DOM text has no actionable refs; add document/frame-bound refs, node-targeted scroll and media resolution |
| `AXAPI.click`, `drag`, `get`, `write`, `paste`, `performSecondaryAction`, `pressKey`, `scroll`, `selectText`, `setValue`, `typeText` | Not extension baseline namespace | `Tab.ax` is excluded by default for extension. CDP AX snapshot/find/click are useful separate features; they do not reproduce AX index/diff state, secondary actions or the official model-display `write()` surface |
| `TabClipboardAPI.read`, `readText`, `write`, `writeText` | Bridge-native partial | `codex_clipboard` stores bounded session-virtual MIME items and integrates Ctrl/Cmd+V paste. It never reads the OS clipboard or invents Ctrl/Cmd+C/X capture |
| `TabDevAPI.logs` | CDP partial | Existing timed capture misses earlier logs and returns raw params; bounded persistent Runtime/Log event storage can provide rendered message, timestamp, level/filter/limit |

Keyboard correctness is shared work: `keys: ["Control", "c"]` means a held
combination, not Control-down/up followed by c-down/up. Copy/cut/paste must use
the virtual session clipboard rather than silently accessing global clipboard
state. Event capture must remain tab-scoped and survive pagination without
silently mixing another profile or document.

## Optional capabilities and exports

| Capability / methods | Mapping | Remaining behavior or boundary |
|---|---|---|
| Browser `management.windows/tabs/tabGroups/bookmarks`, `getAuditTrail` | Native | Existing official restricted management path; preserve denied methods/shared-group restrictions and audit results; never replace denial with raw privileged APIs |
| Browser `viewport.set/reset` | Native | Positive dimensions, explicit temporary override/reset |
| Browser `visibility.get/set` | Conditional native | Expose unsupported builds explicitly; method presence in official docs does not establish extension acceptance |
| Browser/tab capability collection `list/get`, capability `documentation` | Implementable local routing | Return only implemented and usable bridge operations; retain backend-advertised metadata separately |
| Tab `cdp.send/readEvents` | CDP partial | Diagnostic-only send allowlist exists; child target selectors, timeout and bounded sequence/cursor event history remain. Broader raw access requires its own origin/permission contract, not a prefix allowlist |
| Tab `pageAssets.list/bundle` | Bridge-native partial | `codex_page_assets` builds a document-bound inventory, caches bounded content, and atomically writes bundles/manifests through CDP. It is not a native capability passthrough; full official asset taxonomy and browser-visible download semantics remain |
| Tab `webmcp.fetchTools`; returned tools `description/call` | Bridge-native partial | `codex_webmcp` uses `document.modelContext.getTools/executeTool`, document epochs, descriptor checks and stale-handle rejection for the top same-origin document. Isolated worlds, cross-frame tools and official policy remain |
| Tab `browserAuth.request` | App-private exact flow | Secure elicitation, credential broker, protected targets and submission authority require host support. A future MCP client integration needs secure forms and must never return secrets to the model |
| Tab `botDetection.report` | App-private exact telemetry | Codex reporting destination is private; a local blocker report can expose hostname/reason but must not claim Codex telemetry delivery |
| `ContentAPI.export` | Not extension baseline | Default asset-loader/library export is excluded for extension |
| `ContentAPI.exportGsuite` | Bridge-native partial | `codex_content_export` validates Workspace type/format and fetches inside the logged-in page into a bounded, atomic local file. Artifact-library delivery and exact app-side file naming differ |
| `ContentAPI.exportYouTubeTranscript` | Bridge-native partial | `codex_content_export` validates a watch URL, reads the current player transcript and writes bounded UTF-8 text locally; it does not use the ChatGPT library |

ChatGPT library upload, app-owned artifacts, cloud takeover UI, Guardian review,
and enterprise policy decisions are app-private. The bridge-native clipboard,
WebMCP, page-asset and local export implementations are useful but do not
inherit those services, policy, file visibility, or UI behavior.

## Shared implementation priorities

1. Complete the unified locator/input layer: semantic reads, bounded waits,
   native setters, reliable keyboard chords and frame routing. Use the same
   input code for CUA, DOM, locator and future AX actions.
2. Add tab/document-scoped observation state: frame registry, bounded event
   cursor, dialog state, logs and navigation epochs. It supports multiple
   missing interfaces without one polling implementation per tool.
3. Add one controlled file-result layer for page-asset bundles, GSuite and
   transcript exports, then verified downloads/file-chooser integration.
   Enforce roots, byte/count/time limits and browser-versus-bridge host paths.
4. Add virtual clipboard and WebMCP as grouped operations, with MIME limits
   and registration-generation checks. Keep capability discovery truthful.

These are implementation directions, not completed parity claims. Cross-origin
frames, complete ARIA name computation, Playwright actionability/retry behavior,
read-only evaluate and secure auth remain explicit gaps until their contracts
and tests exist.

## Bridge-specific implementations and alignment decisions

These surfaces are intentionally owned by the bridge. They should be named and
documented as bridge contracts even when they produce the same user-visible
outcome as an official app-side adapter.

| Surface | Current contract | Alignment decision |
|---|---|---|
| Compact CLI | `tools`, `call`, JSONL `batch`; one process/session for batch | Keep. It reuses MCP schemas and handlers and is not an official API member |
| Locator | Declarative result spec over CSS/role/text/label/placeholder/test-id | Keep the transport shape; continue aligning action semantics. Do not clone Playwright object chaining or add unsupported compatibility fields |
| DOM/AX diagnostics | AX tree, backend DOM IDs and visible-DOM text are separate | Keep separate names. AX `nodeId`, `backendDOMNodeId` and an official-style actionable visible-DOM ref must never be conflated |
| Pointer input | Shared bounded CDP dispatcher; no mutation replay | Aligned to CUA button/modifier/path fields. `mouseMoved` is applied without an extension response, so each move has a bounded no-ack window and drag latency follows the path length |
| `codex_evaluate` | Unrestricted page JavaScript | Keep as a bridge escape hatch. A read-only locator-scoped evaluate needs a different name, sandbox and result contract |
| `codex_clipboard` | Session-virtual, bounded MIME clipboard | Keep. Do not describe it as the OS clipboard and do not synthesize copy/cut capture |
| `codex_webmcp` | Random document-bound handles, top same-origin document | Keep. A handle is bridge-native identity; the page-defined tool name remains data |
| `codex_page_assets` | Inventory IDs, cached bodies and local bundle/manifest | Keep. This is CDP-equivalent local output, not an extension command passthrough |
| `codex_content_export` | Bounded local GSuite/YouTube files | Keep. It deliberately excludes ChatGPT artifact-library delivery |
| Secure auth, Guardian, approvals, artifacts, telemetry | No local equivalent | Do not inherit or imitate. These require app/host authority and private services |

The main rule is structural: one resolver, one input dispatcher and one
file-result boundary. A legacy tool may remain as a convenience alias, but it
must call the shared implementation rather than maintain a second behavior
engine.

## Tracking an official update

Run the drift checker, compare API members and capability docs, and inspect the
actual native request path before implementing a new operation. Record each
member's input/return differences, extension eligibility, implementation route,
mock tests and live evidence. Refresh baseline hashes only after reviewing the
differences. Keep the official package as developer input; distribute the bridge's
own implementation and guides.
