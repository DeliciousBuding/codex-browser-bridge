# Contributing

Thanks for taking the time to contribute. This is a small project, and issues and PRs are both welcome.

## Reporting bugs

Please open an issue using the bug report template. Include:

- The output of `codex-browser-bridge --version`
- The output of `codex-browser-bridge -mode discover`, with local identifiers redacted
- Codex Desktop version, Chrome version, Windows version
- A minimal reproduction (which tool you called, what arguments, what response)

Public issues must not include screenshots, DOM snapshots, tab titles, full private URLs, logged-in page content, tokens, account IDs, or production admin pages. Use GitHub private vulnerability reporting for sensitive repros.

## Reporting security issues

See [SECURITY.md](SECURITY.md). Please don't file these as public issues.

## Development setup

Requirements:

- Go 1.23+
- Rust 1.82+ for the `rewrite/rust-full` branch
- Windows (the bridge depends on Windows named pipes via `go-winio`)
- Codex Desktop and the Codex Chrome Extension running, if you want to test against a real pipe

Build and test:

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
make build
make test
```

The full test suite is hermetic. It uses `net.Pipe` to simulate the Codex pipe, so you don't need Codex Desktop running to run `go test ./...`.

Rust rewrite branch checks:

```bash
cargo check --locked
cargo test --locked
cargo build --locked --release
```

The Rust binary is written to `target/release/codex-browser-bridge.exe`. Use the same MCP config and pass `--mode mcp` when testing that binary locally.

## Code style

- Run `gofmt`/`goimports` before committing. CI enforces this.
- `make test` runs `go vet ./...` and `go test -race -cover ./...`.
- Run `cargo fmt`, `cargo check --locked`, and `cargo test --locked` for Rust rewrite changes.
- `golangci-lint run` is wired into CI; install it locally with [the official instructions](https://golangci-lint.run/usage/install/) and run it before pushing.

## Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/) prefixes (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, `ci:`).
- One logical change per commit.
- Keep the subject line under 72 characters; put detail in the body.

## Pull requests

- Branch from `main`.
- Reference the related issue in the PR description, if any.
- Add or update tests for behavior changes. The wire-format invariants in `internal/client/browser_rpc_test.go` exist because previous regressions were hard to diagnose. Please don't break them silently.
- Update `CHANGELOG.md` under `## [Unreleased]`.
- Update both `README.md` and `README.zh-CN.md` if you add or remove tools.

## Adding a new MCP tool

There are typically four places to touch:

1. `internal/client/browser.go`: add the client method. If it's a CDP-based tool, use `cdpWithAttach` so the debugger is attached first.
2. `internal/mcp/server.go`: register the tool in `registerTools()` and add a handler that returns `[]Content`.
3. `internal/client/browser_rpc_test.go`: lock in the wire format with a `withRecordingServer`-based test.
4. `internal/mcp/handlers_test.go`: add an integration test that exercises the full client-to-handler path.

Then update the count in `internal/mcp/server_test.go:TestRegisteredToolCount` and document the tool in both READMEs.

## Releasing

Maintainer-only:

1. Bump `npm/package.json` version.
2. Move `## [Unreleased]` notes in `CHANGELOG.md` to the new version section.
3. Open a `release/vX.Y.Z` PR and merge it.
4. Tag from `main`: `git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z`.
5. The release workflow builds Windows binaries (amd64 + arm64), generates checksums, publishes a GitHub Release, embeds those checksums into the npm package, and publishes npm with provenance.
