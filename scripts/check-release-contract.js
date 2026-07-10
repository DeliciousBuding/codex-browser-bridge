#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const root = process.cwd();
const releaseWorkflowPath = path.join(root, ".github", "workflows", "release.yml");
const releaseDocsPath = path.join(root, "docs", "release-process.md");
const packageJsonPath = path.join(root, "npm", "package.json");

const failures = [];

function read(file) {
  return fs.readFileSync(file, "utf8");
}

function fail(message) {
  failures.push(message);
}

function lineMatching(lines, pattern) {
  return lines.findIndex((line) => pattern.test(line));
}

function indentedBlock(lines, startPattern, nextPeerPattern) {
  const start = lineMatching(lines, startPattern);
  if (start < 0) return "";
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (nextPeerPattern.test(lines[index])) {
      end = index;
      break;
    }
  }
  return lines.slice(start, end).join("\n");
}

function collectRunBlocks(lines) {
  const blocks = [];
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^(\s*)run:\s*(.*)$/);
    if (!match) continue;
    const indent = match[1].length;
    const block = [match[2]];
    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      const line = lines[cursor];
      const lineIndent = line.match(/^ */)[0].length;
      if (line.trim() && lineIndent <= indent) break;
      block.push(line);
      index = cursor;
    }
    blocks.push(block.join("\n"));
  }
  return blocks;
}

function countMatches(text, pattern) {
  return [...text.matchAll(pattern)].length;
}

const workflow = read(releaseWorkflowPath);
const workflowLines = workflow.split(/\r?\n/);
const ciWorkflow = read(path.join(root, ".github", "workflows", "ci.yml"));
const supplyChainWorkflow = read(path.join(root, ".github", "workflows", "supply-chain.yml"));
const docs = read(releaseDocsPath);
const pkg = JSON.parse(read(packageJsonPath));

const publishNpm = indentedBlock(workflowLines, /^  publish-npm:/, /^  [A-Za-z0-9_-]+:/);
if (!publishNpm) {
  fail("release.yml is missing the publish-npm job");
} else {
  if (!/permissions:\n(?:    .+\n)*?      id-token: write/.test(publishNpm)) {
    fail("publish-npm job must grant id-token: write for npm Trusted Publishing");
  }
  if (!/permissions:\n(?:    .+\n)*?      contents: read/.test(publishNpm)) {
    fail("publish-npm job should keep contents permission read-only");
  }
  if (/NODE_AUTH_TOKEN|NPM_TOKEN|npm_[A-Za-z0-9]/.test(publishNpm)) {
    fail("publish-npm job must not use token-based npm publishing");
  }
  if (!/node-version:\s*"24"/.test(publishNpm)) {
    fail("publish-npm job should use Node 24 so npm Trusted Publishing support is current");
  }
  if (!/npm >= 11\.5\.1|npm .*too old for Trusted Publishing/.test(publishNpm)) {
    fail("publish-npm job must gate npm >= 11.5.1 before publishing");
  }
  if (!/npm publish --access public --tag/.test(publishNpm)) {
    fail("publish-npm job must publish with explicit public access and dist-tag");
  }
}

if (!/Manual releases must be dispatched from the main branch workflow/.test(workflow)) {
  fail("release.yml must reject workflow_dispatch releases from non-main workflow refs");
}
if (!/Guard published npm checksum drift/.test(workflow)) {
  fail("release.yml must compare published npm checksums before clobbering same-version release assets");
}
if (!/refusing to clobber release asset/.test(workflow)) {
  fail("release.yml must fail when current release asset checksums differ from the published npm package");
}
if (countMatches(workflow, /E404\|404 Not Found\|is not in this registry/g) < 2) {
  fail("release.yml must treat only npm 404 results as unpublished in release and publish jobs");
}

const validateIndex = workflow.indexOf("Validate release tag input");
const firstCheckoutIndex = workflow.indexOf("actions/checkout@");
const firstRepoScriptIndex = workflow.indexOf("node scripts/");
if (validateIndex < 0) {
  fail("release.yml must validate the release tag before checkout");
}
if (validateIndex >= 0 && firstCheckoutIndex >= 0 && validateIndex > firstCheckoutIndex) {
  fail("release.yml must not checkout an unvalidated manual release tag");
}
if (validateIndex >= 0 && firstRepoScriptIndex >= 0 && validateIndex > firstRepoScriptIndex) {
  fail("release.yml must not run repo-local scripts before validating the release tag");
}
if (/ref:\s*\$\{\{\s*(github\.event\.inputs|inputs\.)/.test(workflow)) {
  fail("release.yml checkout refs must come from validated step outputs, not raw workflow inputs");
}

for (const block of collectRunBlocks(workflowLines)) {
  if (/\$\{\{\s*(github\.event\.inputs|inputs\.|github\.ref(?:_name)?\b)/.test(block)) {
    fail("release.yml run blocks must route workflow inputs/ref context through env vars first");
    break;
  }
}

const setupNodeCount = countMatches(workflow, /actions\/setup-node@[0-9a-f]{40}/g);
const setupNodeNoCacheCount = countMatches(workflow, /package-manager-cache:\s*false/g);
if (setupNodeNoCacheCount < setupNodeCount) {
  fail("every release.yml setup-node step must set package-manager-cache: false");
}

for (const [name, text, jobs] of [
  ["release.yml", workflow, ["test", "test-lib", "release", "publish-npm"]],
  ["ci.yml", ciWorkflow, ["rust", "npm", "test-lib"]],
  ["supply-chain.yml", supplyChainWorkflow, ["cargo-deny"]],
]) {
  for (const job of jobs) {
    if (new RegExp(`\\n  ${job}:\\n(?:    .+\\n)*?    runs-on:[^\\n]+\\n(?:    .+\\n)*?    timeout-minutes:`).test(text)) {
      continue;
    }
    fail(`${name} ${job} job must set timeout-minutes`);
  }
}

if (!/tags:\s*\["v\*"\]/.test(workflow)) {
  fail("release.yml must be tag-triggered for v* tags");
}
if (!/gh release create "\$TAG"[\s\S]*--draft/.test(workflow)) {
  fail("release.yml must stage GitHub Release assets as a draft before publishing");
}
if (!/actions\/attest@[0-9a-f]{40}/.test(workflow)) {
  fail("release.yml must generate artifact attestations with a pinned action");
}

if (!/workflow filename:\s*`release\.yml`/.test(docs)) {
  fail("release docs must document the npm Trusted Publisher workflow filename");
}
if (!/allowed action:\s*`npm publish`/.test(docs)) {
  fail("release docs must document the npm Trusted Publisher allowed action");
}
if (!/npm >= 11\.5\.1/.test(docs)) {
  fail("release docs must document the npm CLI Trusted Publishing floor");
}
if (!/package-manager-cache:\s*false/.test(docs)) {
  fail("release docs must document that release setup-node steps disable package-manager-cache");
}
if (!/Manual release dispatch/.test(docs) || !/same branch as `main`/.test(docs)) {
  fail("release docs must document the manual dispatch main-branch restriction");
}
if (!/published npm package's embedded checksums/.test(docs)) {
  fail("release docs must document same-version rerun checksum drift protection");
}

if (pkg.name !== "@delicious233/codex-browser-bridge") {
  fail("npm package name drifted from @delicious233/codex-browser-bridge");
}
if (pkg.private !== false) {
  fail("npm package must remain public (private=false)");
}
if (!pkg.repository || !String(pkg.repository.url).includes("DeliciousBuding/codex-browser-bridge")) {
  fail("npm package repository.url must point at DeliciousBuding/codex-browser-bridge");
}
if (pkg.repository && pkg.repository.directory !== "npm") {
  fail("npm package repository.directory must remain npm");
}

if (failures.length) {
  console.error("Release automation contract check failed:");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log("release contract ok");
