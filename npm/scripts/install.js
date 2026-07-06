#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const https = require("https");

const defaultRepo = "DeliciousBuding/codex-browser-bridge";
const packageRoot = path.join(__dirname, "..");
const binDir = path.join(packageRoot, "bin");
const packageJson = require("../package.json");

function requestBuffer(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { headers: { "User-Agent": "codex-browser-bridge-npm" }, timeout: 60000 }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        requestBuffer(res.headers.location).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}: ${url}`));
        return;
      }
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve(Buffer.concat(chunks)));
    }).on("error", reject);
    req.on("timeout", () => { req.destroy(); reject(new Error(`timeout: ${url}`)); });
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
    const checksums = await fetchBuffer(checksumsURL).catch((err) => {
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
  const skillDir = path.join(root, "skills", "codex-browser");
  if (fs.existsSync(skillDir)) {
    log(`Skill: ${skillDir}\n  → copy to ~/.claude/skills/ to activate the agent skill`);
  }
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
  parseChecksumLine,
  resolveWindowsArch,
  sha256,
};
