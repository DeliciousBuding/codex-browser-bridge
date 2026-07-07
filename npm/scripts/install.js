#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const https = require("https");

const defaultRepo = "DeliciousBuding/codex-browser-bridge";
const packageRoot = path.join(__dirname, "..");
const binDir = path.join(packageRoot, "bin");
const packageJson = require("../package.json");
const DEFAULT_DOWNLOAD_TIMEOUT_MS = 60000;
const DEFAULT_MAX_REDIRECTS = 5;
const DEFAULT_MAX_BYTES = 64 * 1024 * 1024;

function requestBuffer(url, options = {}) {
  const maxBytes = options.maxBytes || DEFAULT_MAX_BYTES;
  const maxRedirects = options.maxRedirects ?? DEFAULT_MAX_REDIRECTS;
  const timeoutMs = options.timeoutMs || DEFAULT_DOWNLOAD_TIMEOUT_MS;
  const get = options.get || https.get;
  return new Promise((resolve, reject) => {
    let settled = false;
    const fail = (err) => {
      if (settled) return;
      settled = true;
      reject(err);
    };
    const req = get(url, { headers: { "User-Agent": "codex-browser-bridge-npm" }, timeout: timeoutMs }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        if (maxRedirects <= 0) {
          fail(new Error(`too many redirects: ${url}`));
          return;
        }
        requestBuffer(new URL(res.headers.location, url).toString(), {
          get,
          maxBytes,
          maxRedirects: maxRedirects - 1,
          timeoutMs,
        }).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        res.resume();
        fail(new Error(`HTTP ${res.statusCode}: ${url}`));
        return;
      }
      const contentLength = Number(res.headers["content-length"]);
      if (Number.isFinite(contentLength) && contentLength > maxBytes) {
        res.resume();
        fail(new Error(`download too large: ${contentLength} bytes (max ${maxBytes})`));
        return;
      }
      const chunks = [];
      let total = 0;
      res.on("data", (c) => {
        total += c.length;
        if (total > maxBytes) {
          req.destroy(new Error(`download too large: exceeded ${maxBytes} bytes`));
          return;
        }
        chunks.push(c);
      });
      res.on("end", () => {
        if (!settled) {
          settled = true;
          resolve(Buffer.concat(chunks));
        }
      });
      res.on("error", fail);
    }).on("error", fail);
    req.on("timeout", () => {
      req.destroy();
      fail(new Error(`timeout: ${url}`));
    });
  });
}

function sha256(buf) {
  return crypto.createHash("sha256").update(buf).digest("hex");
}

function parseChecksumLine(line) {
  const match = line.trim().match(/^([a-fA-F0-9]{64})\s+[*]?(.+)$/);
  if (!match) return null;
  return {
    hash: match[1].toLowerCase(),
    file: match[2].trim(),
  };
}

function findChecksum(text, asset) {
  return text
    .split(/\r?\n/)
    .map(parseChecksumLine)
    .find((entry) => entry && entry.file === asset);
}

function embeddedChecksum(asset, root = packageRoot) {
  const file = path.join(root, "checksums.json");
  if (!fs.existsSync(file)) return null;
  const checksums = JSON.parse(fs.readFileSync(file, "utf8"));
  if (!checksums || !checksums.files || typeof checksums.files[asset] !== "string") {
    return null;
  }
  return {
    hash: checksums.files[asset].toLowerCase(),
    file: asset,
  };
}

function resolveWindowsArch(platform, cpu) {
  if (platform !== "win32") {
    throw new Error("codex-browser-bridge only supports Windows (requires named pipes).");
  }
  switch (cpu) {
    case "x64":
      return "amd64";
    case "arm64":
      return "arm64";
    default:
      throw new Error(`codex-browser-bridge does not ship a Windows binary for ${cpu}.`);
  }
}

function logInstallHints(root, log) {
  const skillDir = path.join(root, "skills", "codex-browser");
  const examplesDir = path.join(root, "examples");
  if (fs.existsSync(skillDir)) {
    log(
      [
        `Skill: ${skillDir}`,
        "  -> Claude Code: copy or symlink to ~/.claude/skills/",
        "  -> Other skill-aware agents: copy or symlink into that agent's skills directory",
      ].join("\n")
    );
  }
  if (fs.existsSync(examplesDir)) {
    log(`MCP config examples: ${examplesDir} (Claude Code, Cursor, OpenClaw, Hermes Agent)`);
  }
}

async function install(options = {}) {
  const platform = options.platform || process.platform;
  const cpu = options.arch || process.arch;
  const env = options.env || process.env;
  const root = options.packageRoot || packageRoot;
  const outDir = options.binDir || binDir;
  const version = options.version || packageJson.version;
  const fetchBuffer = options.requestBuffer || requestBuffer;
  const log = options.log || console.log;

  const arch = resolveWindowsArch(platform, cpu);
  const devDownloads = env.CODEX_BRIDGE_ALLOW_DEV_DOWNLOADS === "1";
  const repo = devDownloads && env.CODEX_BRIDGE_REPO ? env.CODEX_BRIDGE_REPO : defaultRepo;
  const tag = devDownloads && env.CODEX_BRIDGE_TAG ? env.CODEX_BRIDGE_TAG : `v${version}`;
  const exeName = "codex-browser-bridge.exe";
  const asset = arch === "arm64" ? "codex-browser-bridge-arm64.exe" : "codex-browser-bridge.exe";

  const base = `https://github.com/${repo}/releases/download/${tag}`;

  let checksum = embeddedChecksum(asset, root);
  if (!checksum) {
    const checksumsURL = `${base}/checksums.txt`;
    const checksums = await fetchBuffer(checksumsURL, { maxBytes: 1024 * 1024 }).catch((err) => {
      throw new Error(`could not download checksums for ${tag}: ${err.message}`);
    });
    checksum = findChecksum(checksums.toString("utf8"), asset);
    if (!checksum) throw new Error(`checksum not found for ${asset} in ${checksumsURL}`);
  }

  // Download binary
  log(`Downloading codex-browser-bridge ${tag} (${arch})...`);
  const binary = await fetchBuffer(`${base}/${asset}`);

  // Verify checksum
  const actual = sha256(binary);
  if (actual !== checksum.hash) throw new Error(`checksum mismatch: expected ${checksum.hash}, got ${actual}`);

  // Install
  fs.mkdirSync(outDir, { recursive: true });
  const target = path.join(outDir, exeName);
  fs.writeFileSync(target, binary);
  log(`Installed: ${target}`);
  logInstallHints(root, log);
  return { target, asset, tag, repo };
}

async function main() {
  await install();
}

if (require.main === module) {
  main().catch((err) => {
    console.error(`install failed: ${err.message}`);
    process.exit(1);
  });
}

module.exports = {
  embeddedChecksum,
  findChecksum,
  install,
  logInstallHints,
  parseChecksumLine,
  requestBuffer,
  resolveWindowsArch,
  sha256,
};
