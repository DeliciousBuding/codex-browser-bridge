const assert = require("assert");
const { parseChecksumLine, sha256 } = require("./install");

const hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

assert.strictEqual(sha256(Buffer.from("hello")), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
