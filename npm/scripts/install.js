#!/usr/bin/env node

const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const https = require("https");

const repo = process.env.CODEX_BRIDGE_REPO || "DeliciousBuding/codex-browser-bridge";
const binDir = path.join(__dirname, "..", "bin");
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

async function main() {
  if (process.platform !== "win32") {
    console.error("codex-browser-bridge only supports Windows (requires named pipes).");
    process.exit(1);
  }

  let arch;
  switch (process.arch) {
    case "x64":
      arch = "amd64";
      break;
    case "arm64":
      arch = "arm64";
      break;
    default:
      console.error(`codex-browser-bridge does not ship a Windows binary for ${process.arch}.`);
      process.exit(1);
  }

  const tag = process.env.CODEX_BRIDGE_TAG || `v${packageJson.version}`;
  const exeName = "codex-browser-bridge.exe";
  const asset = arch === "arm64" ? "codex-browser-bridge-arm64.exe" : "codex-browser-bridge.exe";

  const base = `https://github.com/${repo}/releases/download/${tag}`;

  // Download checksums
  const checksumsURL = `${base}/checksums.txt`;
  const checksums = await requestBuffer(checksumsURL).catch((err) => {
    throw new Error(`could not download checksums for ${tag}: ${err.message}`);
  });
  const checksum = checksums.toString("utf8")
    .split(/\r?\n/)
    .map(parseChecksumLine)
    .find((entry) => entry && entry.file === asset);
  if (!checksum) throw new Error(`checksum not found for ${asset} in ${checksumsURL}`);

  // Download binary
  console.log(`Downloading codex-browser-bridge ${tag} (${arch})...`);
  const binary = await requestBuffer(`${base}/${asset}`);

  // Verify checksum
  const actual = sha256(binary);
  if (actual !== checksum.hash) throw new Error(`checksum mismatch: expected ${checksum.hash}, got ${actual}`);

  // Install
  fs.mkdirSync(binDir, { recursive: true });
  const target = path.join(binDir, exeName);
  fs.writeFileSync(target, binary);
  console.log(`Installed: ${target}`);
}

main().catch((err) => {
  console.error(`install failed: ${err.message}`);
  process.exit(1);
});
