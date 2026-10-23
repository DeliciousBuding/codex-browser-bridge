#!/usr/bin/env node
"use strict";

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

const BASELINE_PATH = path.join(__dirname, "..", "docs", "official-surface-baseline.json");
const PLUGIN_ROOT = path.join(
  process.env.CODEX_HOME || path.join(os.homedir(), ".codex"),
  "plugins",
  "cache",
  "openai-bundled",
);

function compareVersions(left, right) {
  const a = left.split(/[.-]/).map((part) => Number(part) || 0);
  const b = right.split(/[.-]/).map((part) => Number(part) || 0);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    if ((a[index] || 0) !== (b[index] || 0)) return (a[index] || 0) - (b[index] || 0);
  }
  return 0;
}

function latestPluginDirectory(packageName) {
  const packageRoot = path.join(PLUGIN_ROOT, packageName);
  if (!fs.existsSync(packageRoot)) return null;
  const versions = fs
    .readdirSync(packageRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort(compareVersions);
  const latest = versions.at(-1);
  return latest == null ? null : path.join(packageRoot, latest);
}

function hash(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function packageSurface(pluginDir, expected) {
  return {
    version: path.basename(pluginDir),
    files: Object.fromEntries(
      Object.keys(expected.files).map((relative) => [relative, hash(path.join(pluginDir, relative))]),
    ),
  };
}

function countSurface(pluginDir) {
  const api = JSON.parse(fs.readFileSync(path.join(pluginDir, "docs", "api.json"), "utf8"));
  let methods = 0;
  for (const value of Object.values(api.interfaces || {})) methods += Object.keys(value).length;

  const service = fs.readFileSync(path.join(pluginDir, "scripts", "browser-service.mjs"), "utf8");
  const specialBlock = service.match(/var ng=\{\};H\(ng,\{([\s\S]*?)\}\);/)?.[1] || "";
  const nativeTypes = new Set(
    [...service.matchAll(/(?:var|const|let) ([A-Za-z_$][\w$]*)=L\("([^"]+)"/g)].map(
      (match) => match[2],
    ),
  );

  return {
    interfaces: Object.keys(api.interfaces || {}).length,
    methods,
    specialCommands: (specialBlock.match(/=>/g) || []).length,
    nativeCommandTypes: nativeTypes.size,
  };
}

function main() {
  const baseline = JSON.parse(fs.readFileSync(BASELINE_PATH, "utf8"));
  const directories = Object.fromEntries(
    Object.keys(baseline.packages).map((name) => [name, latestPluginDirectory(name)]),
  );
  if (Object.values(directories).every((directory) => directory == null)) {
    console.log("official browser plugin not installed locally; drift check skipped");
    return;
  }

  const primaryName = directories.browser == null ? "chrome" : "browser";
  const primaryDirectory = directories[primaryName];
  const counts = countSurface(primaryDirectory);
  const extensionCatalog = JSON.parse(
    fs.readFileSync(path.join(primaryDirectory, "scripts", "extension-ids.json"), "utf8"),
  );
  const actual = {
    officialBrowserPluginVersion: path.basename(primaryDirectory),
    packages: Object.fromEntries(
      Object.entries(directories)
        .filter(([, directory]) => directory != null)
        .map(([name, directory]) => [name, packageSurface(directory, baseline.packages[name])]),
    ),
    counts,
    families: {
      names: (extensionCatalog.browserExtensions || [])
        .map((browser) => browser.browserFamily)
        .sort(),
      extensionIds: [...(extensionCatalog.extensionIds || [])].sort(),
      extensionHostName: extensionCatalog.extensionHostName,
    },
  };
  const differences = [];
  if (actual.officialBrowserPluginVersion !== baseline.officialBrowserPluginVersion) {
    differences.push(
      `version ${actual.officialBrowserPluginVersion} != baseline ${baseline.officialBrowserPluginVersion}`,
    );
  }
  for (const [name, expectedPackage] of Object.entries(baseline.packages)) {
    const actualPackage = actual.packages[name];
    if (actualPackage == null) {
      differences.push(`${name} package is not installed`);
      continue;
    }
    if (actualPackage.version !== expectedPackage.version) {
      differences.push(
        `${name} version ${actualPackage.version} != ${expectedPackage.version}`,
      );
    }
    for (const [file, expectedHash] of Object.entries(expectedPackage.files)) {
      if (actualPackage.files[file] !== expectedHash) {
        differences.push(`${name}/${file} changed`);
      }
    }
  }
  for (const [key, expected] of Object.entries(baseline.counts)) {
    if (actual.counts[key] !== expected) {
      differences.push(`${key}: ${actual.counts[key]} != ${expected}`);
    }
  }
  if (JSON.stringify(actual.families) !== JSON.stringify(baseline.families)) {
    differences.push("browser family, extension ID, or native-host catalog changed");
  }

  if (differences.length > 0) {
    console.error(`official browser surface drift detected:\n- ${differences.join("\n- ")}`);
    console.error(
      "Review docs/official-parity.md, update bridge/tests, then refresh docs/official-surface-baseline.json.",
    );
    process.exitCode = 1;
    return;
  }

  console.log(
    `official browser surface matches ${actual.officialBrowserPluginVersion} ` +
      `(${actual.counts.methods} methods, packages: ${Object.keys(actual.packages).join(", ")})`,
  );
}

main();
