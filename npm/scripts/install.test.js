const assert = require("assert");
const { EventEmitter } = require("events");
const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const { PassThrough } = require("stream");
const {
  embeddedChecksum,
  findChecksum,
  install,
  logInstallHints,
  parseChecksumLine,
  requestBuffer,
  resolveWindowsArch,
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
    assert.strictEqual(resolveWindowsArch("win32", "x64"), "amd64");
    assert.strictEqual(resolveWindowsArch("win32", "arm64"), "arm64");
    assert.throws(() => resolveWindowsArch("linux", "x64"), /only supports Windows/);
    assert.throws(() => resolveWindowsArch("win32", "ia32"), /does not ship/);

    await assert.rejects(
      requestBuffer("https://example.test/loop", {
        get: fakeGet(() => ({ statusCode: 302, headers: { location: "/loop" } })),
        maxRedirects: 2,
      }),
      /too many redirects/
    );
    await assert.rejects(
      requestBuffer("https://example.test/large-header", {
        get: fakeGet({ "https://example.test/large-header": { headers: { "content-length": "5" }, body: Buffer.alloc(1) } }),
        maxBytes: 4,
      }),
      /download too large: 5 bytes/
    );
    await assert.rejects(
      requestBuffer("https://example.test/large-body", {
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
      JSON.stringify({ files: { "codex-browser-bridge.exe": binaryHash } })
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
    assert.ok(installLogs.some((line) => line.includes("Skill:") && line.includes(path.join(installRoot, "skills", "codex-browser"))));
    assert.ok(installLogs.some((line) => line.includes("MCP config examples:") && line.includes(path.join(installRoot, "examples"))));

    const hintsRoot = path.join(tmp, "hints-root");
    fs.mkdirSync(path.join(hintsRoot, "skills", "codex-browser"), { recursive: true });
    fs.mkdirSync(path.join(hintsRoot, "examples"), { recursive: true });
    const hintLogs = [];
    logInstallHints(hintsRoot, (line) => hintLogs.push(line));
    const hintText = hintLogs.join("\n");
    assert.match(hintText, /Claude Code/);
    assert.match(hintText, /Other skill-aware agents/);
    assert.match(hintText, /OpenClaw/);
    assert.match(hintText, /Hermes Agent/);
    assert.match(hintText, /examples/);

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

    const wrapperExe = path.join(__dirname, "..", "bin", "codex-browser-bridge.exe");
    if (!fs.existsSync(wrapperExe)) {
      const wrapper = spawnSync(process.execPath, [path.join(__dirname, "..", "bin", "codex-browser-bridge.js")], {
        encoding: "utf8",
      });
      assert.strictEqual(wrapper.status, 1);
      assert.match(wrapper.stderr, /codex-browser-bridge\.exe is missing/);
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
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});
