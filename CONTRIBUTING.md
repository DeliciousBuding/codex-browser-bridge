# Contributing

Thanks for taking the time to contribute.

## Reporting bugs

Please open an issue using the bug report template. Include:

- The output of `codex-browser-bridge --version`
- The output of `codex-browser-bridge --mode discover` for the affected surface, with local identifiers redacted
- Installed runtime and Codex desktop app versions, OS and architecture
- A minimal reproduction (which tool you called, what arguments, what response)

Public issues must not include screenshots, DOM snapshots, tab titles, full private URLs, logged-in page content, tokens, account IDs, or production admin pages. Use GitHub private vulnerability reporting for sensitive repros.

## Reporting security issues

See [SECURITY.md](SECURITY.md) and use the private reporting path there.

## Development setup

Requirements:
- Rust 1.88+
- Windows, or Linux (release GNU binaries require glibc >= 2.39)
- Installed native Codex runtimes (`node_repl` / `cua_repl`) for live relay testing

Build and test:

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
cargo check --locked           # fast check
cargo test --locked             # run all tests
cargo clippy --locked -- -D warnings  # lint
cargo build --locked --release  # release build
```

The default test suite is hermetic. It uses fixture launch rows and mock
workers, so `cargo test --locked` runs without a live desktop app. Linux native
x64/arm64 jobs are configured for full tests, clippy and release builds. macOS
compilation alone does not establish runtime support.

The root directory is an installable Claude Code plugin. Its MCP entry uses
`npx` with the npm package version pinned to the plugin version; keep the pin,
`plugin.json`, and `npm/package.json` aligned. Validate the plugin contract
after changing a manifest, MCP launch config, marketplace, or skill:

```bash
node scripts/check-plugin-surface.js
claude plugin validate --strict .
```

`.claude-plugin/` and `mcp.json` are the Claude Code plugin surface.
Codex / ChatGPT desktop use native browser integration and do not install this
bridge as a plugin. The Claude plugin and manual JSON examples register the
installed Browser Use and Computer Use services separately.

The bridge is a relay: tools, schemas, help text, and permissions come from the
installed runtime. Do not add a bridge-side catalog, profile system, CDP
wrapper, DOM/observation layer, or permission allowlist; changes there would
duplicate the runtime and drift. Smoke-test each supported surface against its
local service separately; WSL does not make Linux workers and Windows workers
interchangeable.

## Code style

- Run `cargo fmt` before committing. CI enforces clippy with zero warnings.
- `cargo test --locked` runs all unit and integration tests.
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
- Update both `README.md` and `README.zh-CN.md` when user-facing behavior changes.

## Changing the relay

Relay work touches a small surface:

1. `src/native.rs`: discovery, environment policy, relay framing, tool policy, or cleanup.
2. `src/cli.rs` / `src/main.rs`: CLI subcommands and flag validation.
3. `tests/` or module tests: add coverage for the new behavior, including the failure path.
4. Both READMEs and `docs/native-computer-use.md` when user-facing behavior changes.

Keep new assertions about real relay behavior — forwarded frames, process
teardown, or policy rejection — rather than matching documentation wording.

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
