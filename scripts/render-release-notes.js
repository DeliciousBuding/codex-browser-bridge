#!/usr/bin/env node
"use strict";

// Renders the GitHub Release body for one version from CHANGELOG.md and
// CHANGELOG.zh-CN.md: English body, Chinese body, one shared Install block, and
// a compare link to the previous SemVer tag.

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const REPO_SLUG = "DeliciousBuding/codex-browser-bridge";
const PACKAGE_NAME = "@delicious233/codex-browser-bridge";
const PLUGIN_NAME = "codex-browser@codex-browser-bridge";
const CHANGELOG = "CHANGELOG.md";
const CHANGELOG_ZH = "CHANGELOG.zh-CN.md";

// First version shipped as the npm package.
const NPM_FIRST_VERSION = "0.1.0";
// Tags whose npm manifest existed but whose package never reached the registry.
const NPM_UNPUBLISHED_VERSIONS = new Set(["1.8.0", "1.9.0"]);
// First version that also shipped the Claude Code plugin.
const PLUGIN_FIRST_VERSION = "1.11.2";

const repoRoot = path.resolve(__dirname, "..");

function fail(message) {
  console.error(`render-release-notes: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = { version: null, prevTag: null, out: null };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      const value = argv[index + 1];
      if (value === undefined || value.startsWith("--")) fail(`${arg} requires a value`);
      index += 1;
      return value;
    };
    switch (arg) {
      case "--version":
        options.version = next();
        break;
      case "--prev-tag":
        options.prevTag = next();
        break;
      case "--out":
        options.out = next();
        break;
      case "--help":
      case "-h":
        console.log("Usage: node scripts/render-release-notes.js --version <X.Y.Z> [--prev-tag <vPrev>] [--out <file>]");
        process.exit(0);
        break;
      default:
        fail(`unknown argument ${arg}`);
    }
  }
  if (!options.version) fail("--version <X.Y.Z> is required");
  options.version = options.version.replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(options.version)) {
    fail(`invalid SemVer version ${options.version}`);
  }
  if (options.prevTag) options.prevTag = options.prevTag.replace(/^v/, "");
  return options;
}

function parseSemver(text) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(text.replace(/^v/, ""));
  if (!match) return null;
  return {
    numbers: [Number(match[1]), Number(match[2]), Number(match[3])],
    prerelease: match[4] ? match[4].split(".") : [],
  };
}

function compareSemver(left, right) {
  const a = parseSemver(left);
  const b = parseSemver(right);
  for (let index = 0; index < 3; index += 1) {
    if (a.numbers[index] !== b.numbers[index]) return a.numbers[index] < b.numbers[index] ? -1 : 1;
  }
  if (!a.prerelease.length && !b.prerelease.length) return 0;
  if (!a.prerelease.length) return 1;
  if (!b.prerelease.length) return -1;
  for (let index = 0; index < Math.max(a.prerelease.length, b.prerelease.length); index += 1) {
    const one = a.prerelease[index];
    const two = b.prerelease[index];
    if (one === undefined) return -1;
    if (two === undefined) return 1;
    if (one === two) continue;
    const oneNumeric = /^\d+$/.test(one);
    const twoNumeric = /^\d+$/.test(two);
    if (oneNumeric && twoNumeric) return Number(one) < Number(two) ? -1 : 1;
    if (oneNumeric !== twoNumeric) return oneNumeric ? -1 : 1;
    return one < two ? -1 : 1;
  }
  return 0;
}

function findPreviousTag(version) {
  let output;
  try {
    output = execFileSync("git", ["tag", "--list", "v*"], { cwd: repoRoot, encoding: "utf8" });
  } catch {
    return null;
  }
  let best = null;
  for (const raw of output.split(/\r?\n/)) {
    const tag = raw.trim();
    if (!tag || !parseSemver(tag)) continue;
    if (compareSemver(tag, version) >= 0) continue;
    if (!best || compareSemver(tag, best) > 0) best = tag;
  }
  return best;
}

function readSection(file, version) {
  const filePath = path.join(repoRoot, file);
  if (!fs.existsSync(filePath)) return null;
  const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => new RegExp(`^## \\[${version.replace(/[.+*?^${}()|[\]\\]/g, "\\$&")}\\] - \\d{4}-\\d{2}-\\d{2}$`).test(line));
  if (start < 0) return null;
  const body = [];
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith("## [")) break;
    body.push(line);
  }
  while (body.length && !body[0].trim()) body.shift();
  while (body.length && !body[body.length - 1].trim()) body.pop();
  return body.length ? body.join("\n") : null;
}

function rewriteLinks(text, tag) {
  return text.replace(/\]\(([^()\s]+)\)/g, (match, target) => {
    if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith("#") || target.startsWith("//")) {
      return match;
    }
    const relative = target.replace(/^\.?\//, "");
    return `](https://github.com/${REPO_SLUG}/blob/${tag}/${relative})`;
  });
}

function installBlock(version) {
  const tag = `v${version}`;
  const hasNpm = compareSemver(version, NPM_FIRST_VERSION) >= 0 && !NPM_UNPUBLISHED_VERSIONS.has(version);
  const hasPlugin = compareSemver(version, PLUGIN_FIRST_VERSION) >= 0;
  const lines = ["### Install", ""];
  if (!hasNpm) {
    lines.push(`This release predates the published npm package. Build from source at tag \`${tag}\` with \`cargo build --release\`.`);
    return lines.join("\n");
  }
  const commands = [`npm install -g ${PACKAGE_NAME}@${version}`];
  if (hasPlugin) {
    commands.push("# or Claude Code plugin");
    commands.push(`claude plugin marketplace add ${REPO_SLUG}`);
    commands.push(`claude plugin install ${PLUGIN_NAME}`);
  }
  lines.push("```bash", ...commands, "```");
  return lines.join("\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const tag = `v${options.version}`;

  const english = readSection(CHANGELOG, options.version);
  if (!english) fail(`${CHANGELOG} is missing a dated section for ${options.version}`);
  const chinese = readSection(CHANGELOG_ZH, options.version);
  if (!chinese) console.error(`render-release-notes: no ${CHANGELOG_ZH} section for ${options.version}; omitting Chinese notes`);

  const parts = [rewriteLinks(english, tag)];
  if (chinese) parts.push(rewriteLinks(chinese, tag));
  parts.push(installBlock(options.version));

  const prevTag = options.prevTag ? `v${options.prevTag}` : findPreviousTag(options.version);
  if (prevTag && compareSemver(prevTag, options.version) < 0) {
    parts.push(`**Full Changelog**: https://github.com/${REPO_SLUG}/compare/${prevTag}...${tag}`);
  } else if (options.version !== "0.1.0") {
    console.error(`render-release-notes: no previous tag before ${tag}; omitting compare link`);
  }

  const notes = `${parts.join("\n\n---\n\n")}\n`;
  if (options.out) {
    fs.writeFileSync(path.resolve(options.out), notes);
  } else {
    process.stdout.write(notes);
  }
}

main();
