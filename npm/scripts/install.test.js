const assert = require("assert");
const { EventEmitter } = require("events");
const { spawnSync } = require("child_process");
const fs = require("fs");
const net = require("net");
const path = require("path");
const { PassThrough } = require("stream");
const {
  buildConnectRequest,
  embeddedChecksum,
  findChecksum,
  install,
  logInstallHints,
  mcpConfigForTarget,
  parseChecksumLine,
  proxyForUrl,
  requestBuffer,
  resolveTarget,
  runtimeGlibc,
  sha256,
} = require("./install");
const { requiredFilesForEnv } = require("./check-package");
const { packageJsonForPublish, stagePackageJson } = require("./prepare-package");

const hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const binary = Buffer.from("fake binary");
const binaryHash = sha256(binary);

assert.deepStrictEqual(parseChecksumLine(`${hash} *codex-browser-bridge.exe`), {
  hash,
  file: "codex-browser-bridge.exe",
});

assert.deepStrictEqual(parseChecksumLine(`${hash}  codex-browser-bridge-arm64.exe`), {
  hash,
  file: "codex-browser-bridge-arm64.exe",
});

assert.strictEqual(parseChecksumLine("not a checksum"), null);
assert.strictEqual(parseChecksumLine(`abc *codex-browser-bridge.exe`), null);

const entries = [
  `${hash} *codex-browser-bridge.exe`,
  `${"f".repeat(64)} *other.exe`,
].map(parseChecksumLine);
assert.strictEqual(entries.find((entry) => entry && entry.file === "missing.exe"), undefined);

assert.deepStrictEqual(findChecksum(entries.map((entry) => `${entry.hash} *${entry.file}`).join("\n"), "codex-browser-bridge.exe"), {
  hash,
  file: "codex-browser-bridge.exe",
});
assert.strictEqual(findChecksum(`${hash} *codex-browser-bridge.exe`, "missing.exe"), undefined);

const tmp = fs.mkdtempSync(path.join(require("os").tmpdir(), "codex-bridge-install-"));

function fakeGet(routes) {
  return (url, _options, callback) => {
    const req = new EventEmitter();
    req.destroy = (err) => {
      if (err) process.nextTick(() => req.emit("error", err));
    };
    process.nextTick(() => {
      const route = typeof routes === "function" ? routes(url) : routes[url];
      if (!route) {
        req.emit("error", new Error(`unexpected URL: ${url}`));
        return;
      }
      const res = new PassThrough();
      res.statusCode = route.statusCode || 200;
      res.headers = route.headers || {};
      callback(res);
      if (route.body !== undefined) {
        res.end(route.body);
      }
    });
    return req;
  };
}

async function run() {
  try {
    fs.writeFileSync(
      path.join(tmp, "checksums.json"),
      JSON.stringify({ files: { "codex-browser-bridge.exe": hash } })
    );
    assert.deepStrictEqual(embeddedChecksum("codex-browser-bridge.exe", tmp), {
      hash,
      file: "codex-browser-bridge.exe",
    });
    assert.strictEqual(embeddedChecksum("missing.exe", tmp), null);

    assert.strictEqual(sha256(Buffer.from("hello")), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    assert.deepStrictEqual(resolveTarget("win32", "x64"), { asset: "codex-browser-bridge.exe", executable: "codex-browser-bridge.exe" });
    assert.deepStrictEqual(resolveTarget("win32", "arm64"), { asset: "codex-browser-bridge-arm64.exe", executable: "codex-browser-bridge.exe" });
    for (const cpu of ["x64", "arm64"]) {
      assert.deepStrictEqual(resolveTarget("linux", cpu, "2.39"), {
        asset: `codex-browser-bridge-linux-${cpu}-gnu`, executable: "codex-browser-bridge",
      });
      assert.doesNotThrow(() => resolveTarget("linux", cpu, "2.40"));
    }
    for (const libc of [undefined, "musl", "2.38", "1.99", "invalid"]) {
      assert.throws(() => resolveTarget("linux", "x64", libc), /require glibc >= 2.39/);
    }
    assert.throws(() => resolveTarget("darwin", "arm64"), /does not ship binaries for darwin/);
    assert.throws(() => resolveTarget("win32", "ia32"), /does not ship/);
    assert.throws(() => resolveTarget("linux", "riscv64", "2.39"), /does not ship/);

    await assert.rejects(
      requestBuffer("https://example.test/loop", {
        env: {},
        get: fakeGet(() => ({ statusCode: 302, headers: { location: "/loop" } })),
        maxRedirects: 2,
      }),
      /too many redirects/
    );
    await assert.rejects(
      requestBuffer("https://example.test/large-header", {
        env: {},
        get: fakeGet({ "https://example.test/large-header": { headers: { "content-length": "5" }, body: Buffer.alloc(1) } }),
        maxBytes: 4,
      }),
      /download too large: 5 bytes/
    );
    await assert.rejects(
      requestBuffer("https://example.test/large-body", {
        env: {},
        get: fakeGet({ "https://example.test/large-body": { body: Buffer.alloc(5) } }),
        maxBytes: 4,
      }),
      /download too large: exceeded 4 bytes/
    );

    assert.deepStrictEqual(requiredFilesForEnv({}), [
      "package.json",
      "README.md",
      "LICENSE",
      "scripts/install.js",
      "bin/codex-browser-bridge.js",
      "examples/README.md",
      "examples/claude-code.json",
      "examples/cursor.json",
      "examples/hermes-agent.json",
      "examples/openclaw.json",
      "skills/codex-browser/SKILL.md",
      "skills/codex-browser/references/tabs-navigation.md",
      "skills/codex-browser/references/dom-input-capture.md",
      "skills/codex-browser/references/network-session.md",
      "skills/codex-browser/references/recovery-safety.md",
    ]);
    assert.deepStrictEqual(requiredFilesForEnv({ CODEX_BRIDGE_REQUIRE_CHECKSUMS: "1" }), [
      "package.json",
      "README.md",
      "LICENSE",
      "scripts/install.js",
      "bin/codex-browser-bridge.js",
      "examples/README.md",
      "examples/claude-code.json",
      "examples/cursor.json",
      "examples/hermes-agent.json",
      "examples/openclaw.json",
      "skills/codex-browser/SKILL.md",
      "skills/codex-browser/references/tabs-navigation.md",
      "skills/codex-browser/references/dom-input-capture.md",
      "skills/codex-browser/references/network-session.md",
      "skills/codex-browser/references/recovery-safety.md",
      "checksums.json",
    ]);
    assert.deepStrictEqual(
      packageJsonForPublish({
        name: "pkg",
        scripts: {
          test: "node test.js",
          prepack: "node prepack.js",
          postpack: "node postpack.js",
          postinstall: "node install.js",
        },
      }).scripts,
      { postinstall: "node install.js" }
    );
    const stagingPackageJsonPath = path.join(tmp, "stage-package.json");
    const stagingPackageJsonBackupPath = path.join(tmp, "stage-package.json.backup");
    fs.writeFileSync(
      stagingPackageJsonPath,
      `${JSON.stringify({
        name: "pkg",
        scripts: {
          test: "node test.js",
          prepack: "node prepack.js",
          postinstall: "node install.js",
        },
      })}\n`
    );
    stagePackageJson({
      packageJsonPath: stagingPackageJsonPath,
      packageJsonBackupPath: stagingPackageJsonBackupPath,
    });
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(stagingPackageJsonPath, "utf8")).scripts, {
      postinstall: "node install.js",
    });
    assert.match(fs.readFileSync(stagingPackageJsonBackupPath, "utf8"), /"prepack"/);
    assert.throws(
      () =>
        stagePackageJson({
          packageJsonPath: stagingPackageJsonPath,
          packageJsonBackupPath: stagingPackageJsonBackupPath,
        }),
      /stale package backup exists/
    );

    const installRoot = path.join(tmp, "install-root");
    const outDir = path.join(tmp, "bin");
    fs.mkdirSync(installRoot);
    fs.mkdirSync(path.join(installRoot, "skills", "codex-browser"), { recursive: true });
    fs.mkdirSync(path.join(installRoot, "examples"), { recursive: true });
    fs.writeFileSync(
      path.join(installRoot, "checksums.json"),
      JSON.stringify({ files: {
        "codex-browser-bridge.exe": binaryHash,
        "codex-browser-bridge-linux-arm64-gnu": binaryHash,
      } })
    );
    const embeddedCalls = [];
    const installLogs = [];
    const embeddedResult = await install({
      platform: "win32",
      arch: "x64",
      env: {},
      packageRoot: installRoot,
      binDir: outDir,
      version: "1.10.0",
      log: (line) => installLogs.push(line),
      requestBuffer: async (url) => {
        embeddedCalls.push(url);
        return binary;
      },
    });
    assert.strictEqual(embeddedResult.asset, "codex-browser-bridge.exe");
    assert.strictEqual(embeddedCalls.length, 1);
    assert.ok(embeddedCalls[0].endsWith("/v1.10.0/codex-browser-bridge.exe"));
    assert.deepStrictEqual(fs.readFileSync(path.join(outDir, "codex-browser-bridge.exe")), binary);
    assert.ok(installLogs.some((line) => line.includes(`Installed: ${path.join(outDir, "codex-browser-bridge.exe")}`)));
    assert.ok(installLogs.some((line) => line.includes("claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge")));
    assert.ok(installLogs.some((line) => line.includes("Codex / ChatGPT desktop already have native browser integration")));
    assert.ok(!installLogs.some((line) => line.includes("codex plugin marketplace add")));
    assert.ok(installLogs.some((line) => line.includes("Skill bundle fallback:") && line.includes(path.join(installRoot, "skills", "codex-browser"))));
    assert.ok(installLogs.some((line) => line.includes("MCP config examples:") && line.includes(path.join(installRoot, "examples"))));
    const installedConfigLog = installLogs.find((line) => line.includes("Ready-to-paste MCP config"));
    assert.ok(installedConfigLog);
    const installedConfig = JSON.parse(installedConfigLog.slice(installedConfigLog.indexOf("{")));
    assert.strictEqual(
      installedConfig.mcpServers["codex-browser"].command,
      path.join(outDir, "codex-browser-bridge.exe")
    );
    assert.deepStrictEqual(installedConfig.mcpServers["codex-browser"].args, ["--mode", "mcp"]);
    assert.strictEqual(installedConfig.mcpServers["codex-browser"].transport, "stdio");
    assert.strictEqual(installedConfig.mcpServers["codex-browser"].env.CODEX_BRIDGE_PROFILE, "basic");

    assert.strictEqual(
      JSON.parse(mcpConfigForTarget("C:\\Tools\\codex-browser-bridge.exe")).mcpServers["codex-browser"].command,
      "C:\\Tools\\codex-browser-bridge.exe"
    );

    const hintsRoot = path.join(tmp, "hints-root");
    fs.mkdirSync(path.join(hintsRoot, "skills", "codex-browser"), { recursive: true });
    fs.mkdirSync(path.join(hintsRoot, "examples"), { recursive: true });
    const hintLogs = [];
    logInstallHints(hintsRoot, (line) => hintLogs.push(line), undefined, "win32");
    const hintText = hintLogs.join("\n");
    assert.match(hintText, /Claude Code/);
    assert.match(hintText, /claude plugin install codex-browser/);
    assert.match(hintText, /npx/);
    assert.match(hintText, /native browser integration/);
    assert.doesNotMatch(hintText, /codex plugin marketplace add/);
    assert.match(hintText, /Skill bundle fallback/);
    assert.match(hintText, /Other skill-aware agents/);
    assert.match(hintText, /OpenClaw/);
    assert.match(hintText, /Hermes Agent/);
    assert.match(hintText, /examples/);
    assert.match(hintText, /where\.exe codex-browser-bridge/);
    const linuxHints = [];
    logInstallHints(hintsRoot, (line) => linuxHints.push(line), undefined, "linux");
    assert.match(linuxHints.join("\n"), /command -v codex-browser-bridge/);
    assert.doesNotMatch(linuxHints.join("\n"), /PowerShell|where\.exe/);

    const fetchedRoot = path.join(tmp, "fetched-root");
    const fetchedOut = path.join(tmp, "fetched-bin");
    fs.mkdirSync(fetchedRoot);
    const fetchedCalls = [];
    await install({
      platform: "win32",
      arch: "arm64",
      env: {
        CODEX_BRIDGE_ALLOW_DEV_DOWNLOADS: "1",
        CODEX_BRIDGE_REPO: "owner/repo",
        CODEX_BRIDGE_TAG: "v9.9.9-test.1",
      },
      packageRoot: fetchedRoot,
      binDir: fetchedOut,
      version: "1.10.0",
      log: () => {},
      requestBuffer: async (url) => {
        fetchedCalls.push(url);
        if (url.endsWith("checksums.txt")) {
          return Buffer.from(`${binaryHash} *codex-browser-bridge-arm64.exe\n`);
        }
        return binary;
      },
    });
    assert.deepStrictEqual(fetchedCalls, [
      "https://github.com/owner/repo/releases/download/v9.9.9-test.1/checksums.txt",
      "https://github.com/owner/repo/releases/download/v9.9.9-test.1/codex-browser-bridge-arm64.exe",
    ]);

    for (const cpu of ["x64", "arm64"]) {
      const linuxOut = path.join(tmp, `linux-${cpu}`);
      const calls = [];
      const result = await install({
        platform: "linux", arch: cpu, glibcVersion: "2.39", env: {},
        packageRoot: cpu === "arm64" ? installRoot : fetchedRoot,
        binDir: linuxOut, version: "1.10.0", log: () => {},
        requestBuffer: async (url) => {
          calls.push(url);
          return url.endsWith("checksums.txt") ? Buffer.from(`${binaryHash} *codex-browser-bridge-linux-${cpu}-gnu\n`) : binary;
        },
      });
      assert.strictEqual(result.asset, `codex-browser-bridge-linux-${cpu}-gnu`);
      assert.deepStrictEqual(calls, [
        ...(cpu === "arm64" ? [] : ["https://github.com/DeliciousBuding/codex-browser-bridge/releases/download/v1.10.0/checksums.txt"]),
        `https://github.com/DeliciousBuding/codex-browser-bridge/releases/download/v1.10.0/codex-browser-bridge-linux-${cpu}-gnu`,
      ]);
      assert.deepStrictEqual(fs.readFileSync(path.join(linuxOut, "codex-browser-bridge")), binary);
      if (process.platform !== "win32") assert.strictEqual(fs.statSync(result.target).mode & 0o777, 0o755);
    }
    await assert.rejects(install({
      platform: "linux", arch: "x64", glibcVersion: "2.38", env: {},
      requestBuffer: async () => { throw new Error("must not download on an unsupported libc"); },
    }), /require glibc >= 2.39/);

    const nativeName = process.platform === "win32" ? "codex-browser-bridge.exe" : "codex-browser-bridge";
    let nativeSupported = true;
    try { resolveTarget(process.platform, process.arch, runtimeGlibc()); } catch { nativeSupported = false; }
    const wrapperExe = path.join(__dirname, "..", "bin", nativeName);
    if (!fs.existsSync(wrapperExe)) {
      const wrapper = spawnSync(process.execPath, [path.join(__dirname, "..", "bin", "codex-browser-bridge.js")], {
        encoding: "utf8",
      });
      assert.strictEqual(wrapper.status, 1);
      if (process.platform === "linux" && !nativeSupported) {
        assert.match(wrapper.stderr, /require glibc >= 2.39/);
      } else {
        assert.ok(wrapper.stderr.includes(`${nativeName} is missing`), wrapper.stderr);
      }
    }

    // Exercise the real Unix wrapper without downloading/running a release.
    // The isolated shell fixture proves executable selection, argv, and exit
    // propagation, including an argument that must not be split by a shell.
    if (process.platform === "linux" && nativeSupported) {
      const wrapperRoot = path.join(tmp, "wrapper-root");
      fs.mkdirSync(path.join(wrapperRoot, "bin"), { recursive: true });
      fs.mkdirSync(path.join(wrapperRoot, "scripts"));
      fs.copyFileSync(path.join(__dirname, "..", "package.json"), path.join(wrapperRoot, "package.json"));
      fs.copyFileSync(path.join(__dirname, "install.js"), path.join(wrapperRoot, "scripts", "install.js"));
      fs.copyFileSync(path.join(__dirname, "..", "bin", "codex-browser-bridge.js"), path.join(wrapperRoot, "bin", "codex-browser-bridge.js"));
      fs.writeFileSync(path.join(wrapperRoot, "bin", "codex-browser-bridge"), '#!/bin/sh\nprintf \'%s\\n\' "$@"\nexit 7\n', { mode: 0o755 });
      const wrapper = spawnSync(process.execPath, [path.join(wrapperRoot, "bin", "codex-browser-bridge.js"), "--mode", "argument with spaces"], { encoding: "utf8" });
      assert.strictEqual(wrapper.status, 7, wrapper.stderr);
      assert.strictEqual(wrapper.stdout, "--mode\nargument with spaces\n");
    }

    await assert.rejects(
      install({
        platform: "win32",
        arch: "x64",
        env: {},
        packageRoot: fetchedRoot,
        binDir: path.join(tmp, "bad-bin"),
        version: "1.10.0",
        log: () => {},
        requestBuffer: async (url) => (url.endsWith("checksums.txt") ? Buffer.from(`${hash} *codex-browser-bridge.exe\n`) : binary),
      }),
      /checksum mismatch/
    );

    // ── Proxy support ──
    assert.strictEqual(proxyForUrl("https://github.com/x", {}), null);
    assert.strictEqual(proxyForUrl("http://github.com/x", { HTTPS_PROXY: "http://127.0.0.1:7897" }), null);
    assert.deepStrictEqual(proxyForUrl("https://github.com/x", { HTTPS_PROXY: "http://127.0.0.1:7897" }), {
      host: "127.0.0.1",
      port: 7897,
    });
    assert.deepStrictEqual(proxyForUrl("https://github.com/x", { https_proxy: "127.0.0.1:7897" }), {
      host: "127.0.0.1",
      port: 7897,
    });
    assert.deepStrictEqual(proxyForUrl("https://github.com/x", { ALL_PROXY: "http://proxy.local:8080" }), {
      host: "proxy.local",
      port: 8080,
    });
    assert.strictEqual(proxyForUrl("https://github.com/x", { HTTPS_PROXY: "http://127.0.0.1:7897", NO_PROXY: "*" }), null);
    assert.strictEqual(proxyForUrl("https://api.github.com/x", { HTTPS_PROXY: "http://127.0.0.1:7897", NO_PROXY: ".github.com" }), null);
    assert.deepStrictEqual(
      proxyForUrl("https://objects.githubusercontent.com/x", { HTTPS_PROXY: "http://127.0.0.1:7897", NO_PROXY: ".github.com" }),
      { host: "127.0.0.1", port: 7897 }
    );
    // TLS-terminating proxies are out of scope for the installer.
    assert.strictEqual(proxyForUrl("https://github.com/x", { HTTPS_PROXY: "https://127.0.0.1:7897" }), null);

    assert.strictEqual(
      buildConnectRequest("github.com", 443),
      "CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\nProxy-Connection: keep-alive\r\n\r\n"
    );

    // A proxy that rejects CONNECT must surface as a download error, not a hang.
    const rejectingProxy = net.createServer((socket) => {
      socket.once("data", () => socket.write("HTTP/1.1 403 Forbidden\r\n\r\n"));
    });
    await new Promise((resolve) => rejectingProxy.listen(0, "127.0.0.1", resolve));
    try {
      await assert.rejects(
        requestBuffer("https://github.com/example/asset", {
          env: { HTTPS_PROXY: `http://127.0.0.1:${rejectingProxy.address().port}` },
          timeoutMs: 5000,
        }),
        /CONNECT rejected/
      );
    } finally {
      rejectingProxy.close();
    }
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});
