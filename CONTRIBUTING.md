# Contributing

Thanks for taking the time to contribute.

## Reporting bugs

Please open an issue using the bug report template. Include:

- The output of `codex-browser-bridge --version`
- The output of `codex-browser-bridge --mode discover`, with local identifiers redacted
- Codex Desktop version, Chrome version, Windows version
- A minimal reproduction (which tool you called, what arguments, what response)

Public issues must not include screenshots, DOM snapshots, tab titles, full private URLs, logged-in page content, tokens, account IDs, or production admin pages. Use GitHub private vulnerability reporting for sensitive repros.

## Reporting security issues

See [SECURITY.md](SECURITY.md) and use the private reporting path there.

## Development setup

Requirements:
- Rust 1.85+
- Windows (the bridge depends on Windows named pipes)
- Codex Desktop and the Codex Chrome Extension running (for real-pipe testing)

Build and test:

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
cargo check --locked           # fast check
cargo test --locked             # run all tests
cargo clippy --locked -- -D warnings  # lint
cargo build --locked --release  # release build
```

The default test suite is hermetic. It uses mock pipe streams to simulate the Codex pipe, so `cargo test --locked` runs without Codex Desktop. Non-Windows mock E2E tests in `tests/cdp_tools_e2e.rs` run in the Ubuntu CI harness. Optional live E2E against a real Codex Desktop + Chrome session is available with:

```powershell
.\scripts\live-e2e.ps1 -Url https://example.com
```

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

1. Bump version in `Cargo.toml` and `npm/package.json`.
2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to `CHANGELOG.md`.
3. Open a `release/vX.Y.Z` PR and merge it.
4. Tag from `main`: `git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z`.
5. The release workflow builds Windows binaries (x64 + arm64), generates checksums, publishes a GitHub Release, embeds those checksums into the npm package, and publishes npm with provenance.
