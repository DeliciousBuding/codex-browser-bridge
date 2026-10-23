# Contributing

Thanks for taking the time to contribute.

## Reporting bugs

Please open an issue using the bug report template. Include:

- The output of `codex-browser-bridge --version`
- The output of `codex-browser-bridge --mode discover`, with local identifiers redacted
- ChatGPT desktop app/service version, browser and extension versions, OS and architecture
- A minimal reproduction (which tool you called, what arguments, what response)

Public issues must not include screenshots, DOM snapshots, tab titles, full private URLs, logged-in page content, tokens, account IDs, or production admin pages. Use GitHub private vulnerability reporting for sensitive repros.

## Reporting security issues

See [SECURITY.md](SECURITY.md) and use the private reporting path there.

## Development setup

Requirements:
- Rust 1.85+
- Windows, or Linux for the Unix transport (release GNU binaries require glibc >= 2.39)
- A local ChatGPT browser-extension service and connected extension for live testing

Build and test:

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
cargo check --locked           # fast check
cargo test --locked             # run all tests
cargo clippy --locked -- -D warnings  # lint
cargo build --locked --release  # release build
```

The default test suite is hermetic. It uses mock streams and temporary local
sockets, so `cargo test --locked` runs without a live desktop app. Linux native
x64/arm64 jobs are configured for full tests, clippy and release builds. They
test transport behavior, not a live browser extension. macOS compilation alone
does not establish runtime support. Optional Windows live E2E against the
ChatGPT desktop app and a connected Chromium browser is available with:

```powershell
.\scripts\live-e2e.ps1 -Url https://example.com
```

The root directory is an installable Claude Code plugin. Its MCP entry uses
`npx` with the npm package version pinned to the plugin version; keep the pin,
`plugin.json`, and `npm/package.json` aligned. Validate the plugin contract
after changing a manifest, MCP launch config, marketplace, skill, or tool
profile:

```bash
node scripts/check-plugin-surface.js
node scripts/check-agent-surface.js
claude plugin validate --strict .
```

`.claude-plugin/` and `mcp.json` are the Claude Code plugin surface.
Codex / ChatGPT desktop use native browser integration and do not install this
bridge as a plugin. The Claude plugin and manual JSON examples default to the
full 64-tool source profile. Clients without tool search can opt into `basic`
(45 tools) or `network` (63 tools), with the corresponding capability reduction.

Keep method-level coverage in `docs/official-parity.md` accurate. Added tools or
CLI entrypoints do not establish parity with official locator, session, or
app-private semantics. Smoke-test each supported transport against its local
service separately; WSL does not make Linux sockets and Windows pipes interchangeable.

## Code style

- Run `cargo fmt` before committing. CI enforces clippy with zero warnings.
- `cargo test --locked` runs all unit, integration, and e2e tests.
- `cargo clippy --locked -- -D warnings` is wired into CI; run it locally before pushing.

## Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/) prefixes (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, `ci:`).
- One logical change per commit.
- Keep the subject line under 72 characters; put detail in the body.

## Pull requests

- Branch from `main`.
- Reference the related issue in the PR description, if any.
- Add or update tests for behavior changes.
- Update `CHANGELOG.md` with a release-ready section when preparing a release.
- Update both `README.md` and `README.zh-CN.md` if you add or remove tools.

## Adding a new MCP tool

Four places to touch:

1. `src/browser.rs`: add the browser helper (CDP wrapper, parsing logic)
2. `src/mcp/types.rs`, `src/mcp/handlers.rs`, `src/mcp/schema.rs`, and `src/mcp/profiles.rs`: add the handler variant, dispatch, schema, and profile membership
3. `tests/` or module tests: add coverage for parameter validation, CDP response parsing, and mock transport behavior where practical
4. `src/browser.rs` test module: add unit tests for CDP response parsing

Then document the tool in both READMEs and update `CHANGELOG.md`.

## Releasing

Maintainer-only. See [docs/release-process.md](docs/release-process.md) for the full contract.

1. Bump the version in `Cargo.toml`, `npm/package.json`, and
   `.claude-plugin/plugin.json`.
2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to `CHANGELOG.md`.
3. Run the local release preflight in `docs/release-process.md`, including
   plugin validation and npm package-content checks.
4. Commit the release directly on `main` and push it.
5. Tag from `main`: `git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z`.
6. The release workflow builds Windows and Linux GNU binaries
   (x64 + arm64; Linux glibc >= 2.39), generates
   checksums and attestations, stages and publishes the GitHub Release, embeds
   those checksums into the npm package, then publishes npm with provenance. If
   a same-version rerun happens after npm is already published, the workflow
   refuses to clobber Release assets unless the rebuilt hashes match the
   published npm package checksums.
