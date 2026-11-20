# npm and CI Plan

This branch keeps the npm package asset names unchanged while the release workflow moves to Rust-built binaries.

## Current npm Behavior

`npm/scripts/install.js` downloads the release assets used by the published package:

- `codex-browser-bridge.exe` for Windows x64
- `codex-browser-bridge-arm64.exe` for Windows arm64
- `checksums.json` from the package, with `checksums.txt` from the GitHub Release as fallback

Keep `npm/package.json` version unchanged during rewrite work. Publish after the Rust release assets are built and verified from a tag.

## Local Rust Build

Use these commands from the repository root:

```bash
cargo check --locked
cargo test --locked
cargo build --locked --release --target x86_64-pc-windows-msvc
cargo build --locked --release --target aarch64-pc-windows-msvc
```

The Rust binary is generated at:

```text
target/release/codex-browser-bridge.exe
```

For local MCP testing, point the client at that absolute path and keep the same arguments:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "D:/path/to/codex-browser-bridge/target/release/codex-browser-bridge.exe",
      "args": ["--mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

## CI Draft

The `ci.yml` workflow has a `rust` job on `windows-latest`:

```bash
cargo check --locked
cargo test --locked
cargo build --locked --release
```

The existing Go and npm jobs remain in place during the rewrite branch.

## Release Checks

Release and npm publish require these checks:

- Rust tests cover the wire-format, browser API, MCP, CLI compatibility, and pipe discovery cases listed in `design.md`.
- CI builds Windows x64 and arm64 Rust binaries.
- GitHub Release uploads Rust binaries with the same asset names npm already expects.
- Release validation checks that `Cargo.toml` and `npm/package.json` match the tag.
- `npm test` passes with embedded checksums for both asset names.
- A manual install from a draft release validates checksum download, binary placement, and `--version`.
