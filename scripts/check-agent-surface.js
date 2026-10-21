#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const repoRoot = path.resolve(__dirname, "..");
const expectedExamples = [
  "claude-code.json",
  "cursor.json",
  "hermes-agent.json",
  "openclaw.json",
];

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function readSkillBundle() {
  const skillRoot = path.join(
    repoRoot,
    "skills",
    "codex-browser"
  );
  const files = [path.join(skillRoot, "SKILL.md")];
  const references = path.join(skillRoot, "references");
  if (fs.existsSync(references)) {
    for (const entry of fs.readdirSync(references).sort()) {
      if (entry.endsWith(".md")) files.push(path.join(references, entry));
    }
  }
  return files.map((file) => fs.readFileSync(file, "utf8")).join("\n");
}

function fail(message) {
  throw new Error(message);
}

function unique(values) {
  return [...new Set(values)];
}

function extractToolNames() {
  const schema = read("src/mcp/schema.rs");
  return [...schema.matchAll(/Tool::new\("([^"]+)"/g)].map((match) => match[1]);
}

function extractProfileTools(profileName) {
  const profiles = read("src/mcp/profiles.rs");
  const match = profiles.match(
    new RegExp(`const ${profileName}_TOOLS: &\\[&str\\] = &\\[([\\s\\S]*?)\\];`)
  );
  if (!match) {
    fail(`missing ${profileName}_TOOLS in src/mcp/profiles.rs`);
  }
  return [...match[1].matchAll(/"([^"]+)"/g)].map((item) => item[1]);
}

function assertContains(text, needle, label) {
  if (!text.includes(needle)) {
    fail(`${label} is missing ${needle}`);
  }
}

function assertToolCoverage(relativePath, toolNames) {
  const text = read(relativePath);
  assertToolCoverageInText(relativePath, text, toolNames);
}

function assertToolCoverageInText(label, text, toolNames) {
  const missing = toolNames.filter((name) => !text.includes(`\`${name}\``));
  if (missing.length > 0) {
    fail(`${label} is missing tool docs: ${missing.join(", ")}`);
  }
}

function assertProfileCounts(relativePath, counts) {
  const text = read(relativePath);
  assertProfileCountsInText(relativePath, text, counts);
}

function assertProfileCountsInText(label, text, counts) {
  for (const [profile, count] of Object.entries(counts)) {
    const pattern = new RegExp(`\\b${profile}\\b[\\s\\S]{0,80}\\b${count}\\b`);
    if (!pattern.test(text)) {
      fail(`${label} is missing ${profile} profile count ${count}`);
    }
  }
}

function assertExamples() {
  const readme = read("examples/README.md");
  for (const example of expectedExamples) {
    const config = JSON.parse(read(path.join("examples", example)));
    const server = config.mcpServers && config.mcpServers["codex-browser"];
    if (!server) {
      fail(`${example} must define mcpServers.codex-browser`);
    }
    if (server.command !== "codex-browser-bridge") {
      fail(`${example} must use codex-browser-bridge as the command`);
    }
    if (!Array.isArray(server.args) || !server.args.includes("--mode")) {
      fail(`${example} must include --mode in args`);
    }
    if (!server.env || !server.env.CODEX_BRIDGE_PROFILE) {
      fail(`${example} must set CODEX_BRIDGE_PROFILE`);
    }
    if (server.env.CODEX_BRIDGE_PROFILE !== "full") {
      fail(`${example} must expose the documented full tool surface by default`);
    }
    if (!server.env.CODEX_BRIDGE_UPLOAD_BASE) {
      fail(`${example} must document CODEX_BRIDGE_UPLOAD_BASE`);
    }
    assertContains(readme, `[${example}](${example})`, "examples/README.md");
  }
}

function assertNpmPackageFiles() {
  const pkg = JSON.parse(read("npm/package.json"));
  const files = pkg.files || [];
  for (const required of ["examples/", "skills/"]) {
    if (!files.includes(required)) {
      fail(`npm/package.json files must include ${required}`);
    }
  }
}

function main() {
  const toolNames = extractToolNames();
  const uniqueToolNames = unique(toolNames);
  if (toolNames.length !== uniqueToolNames.length) {
    fail("src/mcp/schema.rs contains duplicate tool names");
  }

  const basicTools = extractProfileTools("BASIC");
  const networkTools = extractProfileTools("NETWORK");
  const counts = {
    basic: basicTools.length,
    network: networkTools.length,
    full: uniqueToolNames.length,
  };

  for (const [profile, names] of [
    ["basic", basicTools],
    ["network", networkTools],
  ]) {
    const unknown = names.filter((name) => !uniqueToolNames.includes(name));
    if (unknown.length > 0) {
      fail(`${profile} profile contains unknown tools: ${unknown.join(", ")}`);
    }
  }

  for (const doc of ["README.md", "README.zh-CN.md"]) {
    assertToolCoverage(doc, uniqueToolNames);
    assertProfileCounts(doc, counts);
  }
  const skillBundle = readSkillBundle();
  assertToolCoverageInText("codex-browser skill bundle", skillBundle, uniqueToolNames);
  assertProfileCountsInText("codex-browser skill bundle", skillBundle, counts);

  assertExamples();
  assertNpmPackageFiles();

  console.log(
    `agent surface ok (${counts.full} tools, profiles: basic=${counts.basic}, network=${counts.network}, full=${counts.full})`
  );
}

if (require.main === module) {
  main();
}

module.exports = {
  extractToolNames,
  extractProfileTools,
};
