#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const failures = [];

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(root, relativePath), "utf8"));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function requireFile(relativePath) {
  if (!fs.existsSync(path.join(root, relativePath))) {
    failures.push(`missing ${relativePath}`);
  }
}

function requireAbsent(relativePath, reason) {
  if (fs.existsSync(path.join(root, relativePath))) {
    failures.push(`${relativePath} must stay absent: ${reason}`);
  }
}

function requirePathInsidePlugin(relativePath, label) {
  const normalized = path.posix.normalize(relativePath);
  if (!relativePath.startsWith("./") || normalized.split("/").includes("..")) {
    failures.push(`${label} must be a ./-prefixed path inside the plugin`);
    return;
  }
  requireFile(normalized);
}

function requireVersion(manifest, label, expectedVersion) {
  if (manifest.version !== expectedVersion) {
    failures.push(
      `${label} version ${manifest.version || "<missing>"} does not match ${expectedVersion}`
    );
  }
}

const packageJson = readJson("npm/package.json");
const cargoVersion = readText("Cargo.toml").match(
  /^version\s*=\s*"([^"]+)"/m
);
if (!cargoVersion) {
  failures.push("Cargo.toml must declare a package version");
} else if (cargoVersion[1] !== packageJson.version) {
  failures.push(
    `Cargo.toml version ${cargoVersion[1]} does not match npm version ${packageJson.version}`
  );
}

const portable = readJson("plugin.json");
const codex = readJson(".codex-plugin/plugin.json");
const claude = readJson(".claude-plugin/plugin.json");
const mcp = readJson("mcp.json");
const claudeMarketplace = readJson(".claude-plugin/marketplace.json");
const codexMarketplace = readJson(".agents/plugins/marketplace.json");

for (const [manifest, label] of [
  [portable, "portable plugin.json"],
  [codex, ".codex-plugin/plugin.json"],
  [claude, ".claude-plugin/plugin.json"],
]) {
  if (manifest.name !== "codex-browser") {
    failures.push(`${label} name must remain codex-browser`);
  }
  requireVersion(manifest, label, packageJson.version);
}

const portableSchema = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
if (portable.$schema !== portableSchema) {
  failures.push(`portable plugin.json must use ${portableSchema}`);
}
if (!portable.author || portable.author.name !== "DeliciousBuding") {
  failures.push("portable plugin.json author.name must remain DeliciousBuding");
}
const portableOpenAi =
  portable.extensions && portable.extensions["com.openai"];
if (
  !portableOpenAi ||
  !portableOpenAi.interface ||
  portableOpenAi.interface.developerName !== "DeliciousBuding"
) {
  failures.push(
    "portable plugin.json developerName must remain DeliciousBuding"
  );
}
for (const [manifest, label] of [
  [codex, ".codex-plugin/plugin.json"],
  [claude, ".claude-plugin/plugin.json"],
]) {
  requirePathInsidePlugin(manifest.skills, `${label} skills`);
  requirePathInsidePlugin(manifest.mcpServers, `${label} mcpServers`);
}
if (claude.author && claude.author.name !== "DeliciousBuding") {
  failures.push(".claude-plugin/plugin.json author.name must remain DeliciousBuding");
}
if (!codex.author || codex.author.name !== "DeliciousBuding") {
  failures.push(".codex-plugin/plugin.json author.name must remain DeliciousBuding");
}

const mcpSchema = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";
if (mcp.$schema !== mcpSchema) {
  failures.push(`mcp.json must use ${mcpSchema}`);
}
if (claude.mcpServers !== "./mcp.json") {
  failures.push(
    "Claude manifest must reuse portable mcp.json to avoid a duplicate .mcp.json server"
  );
}
requireAbsent(
  ".mcp.json",
  "a project-level copy makes Claude start the plugin MCP twice"
);

const server = mcp.mcpServers && mcp.mcpServers["codex-browser"];
if (!server) {
  failures.push("mcp.json must define mcpServers.codex-browser");
} else {
  if (server.type !== "stdio") failures.push("plugin MCP transport must be stdio");
  if (server.command !== "codex-browser-bridge") {
    failures.push("plugin MCP command must be codex-browser-bridge");
  }
  if (!Array.isArray(server.args) || server.args.join(" ") !== "--mode mcp") {
    failures.push("plugin MCP args must be --mode mcp");
  }
  if (!server.env || server.env.CODEX_BRIDGE_PROFILE !== "full") {
    failures.push("plugin MCP profile must default to full for tool-search clients");
  }
}

if (!portable.extensions || !portable.extensions["com.openai"]) {
  failures.push("portable plugin must declare extensions.com.openai");
} else if (portable.extensions["com.openai"].requires_local_executor !== true) {
  failures.push("portable plugin must require a local executor");
}
if (codex.requires_local_executor !== true) {
  failures.push("Codex compatibility manifest must require a local executor");
}

const claudeEntry = claudeMarketplace.plugins && claudeMarketplace.plugins[0];
if (!claudeEntry || claudeEntry.name !== "codex-browser") {
  failures.push("Claude marketplace must list codex-browser");
} else if (claudeEntry.source !== "./") {
  failures.push("Claude marketplace source must point at the plugin repository root");
}
if (
  !claudeMarketplace.owner ||
  claudeMarketplace.owner.name !== "DeliciousBuding"
) {
  failures.push("Claude marketplace owner must remain DeliciousBuding");
}

const codexEntry = codexMarketplace.plugins && codexMarketplace.plugins[0];
if (!codexEntry || codexEntry.name !== "codex-browser") {
  failures.push("Codex marketplace must list codex-browser");
} else if (
  !codexEntry.source ||
  codexEntry.source.source !== "local" ||
  codexEntry.source.path !== "./"
) {
  failures.push("Codex marketplace source must point at the plugin repository root");
}

const skillRoot = path.join(root, "skills", "codex-browser");
requireFile("skills/codex-browser/SKILL.md");
for (const reference of fs.readdirSync(path.join(skillRoot, "references"))) {
  if (reference.endsWith(".md")) {
    requireFile(path.join("skills", "codex-browser", "references", reference));
  }
}

if (failures.length) {
  console.error("Plugin surface check failed:");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log(`plugin surface ok (${packageJson.version}, cross-surface package)`);
