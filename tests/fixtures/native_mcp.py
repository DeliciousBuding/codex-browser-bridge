"""Synthetic stdio MCP server with an owned descendant that survives EOF."""
import json
import os
import pathlib
import subprocess
import sys
import time

root = pathlib.Path(os.environ["BRIDGE_NATIVE_FIXTURE_ROOT"])
release = root / "release"
if "--descendant" in sys.argv:
    while not release.exists():
        time.sleep(0.05)
    sys.exit(0)

child = subprocess.Popen(
    [sys.executable, str(pathlib.Path(__file__).resolve()), "--descendant"],
    stdin=subprocess.DEVNULL,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
(root / "pids.json").write_text(json.dumps([os.getpid(), child.pid]))
(root / "cwd.txt").write_text(str(pathlib.Path.cwd()))
for line in sys.stdin:
    row = json.loads(line)
    with (root / "frames.jsonl").open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(row) + "\n")
    if "id" not in row:
        continue
    if row["method"] == "initialize":
        if os.environ.get("BRIDGE_NATIVE_FIXTURE_HANG_INITIALIZE"):
            continue
        result = {
            "protocolVersion": row["params"]["protocolVersion"],
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "synthetic-native", "version": "1"},
        }
    elif row["method"] == "tools/list":
        if os.environ.get("BRIDGE_NATIVE_FIXTURE_HANG_CATALOG"):
            continue
        result = {"tools": []}
    else:
        if row.get("params", {}).get("name") == "js" and os.environ.get(
            "BRIDGE_NATIVE_FIXTURE_HANG_JS"
        ):
            continue
        result = {"content": [{"type": "text", "text": "synthetic response"}]}
    print(json.dumps({"jsonrpc": "2.0", "id": row["id"], "result": result}), flush=True)
if os.environ.get("BRIDGE_NATIVE_FIXTURE_EXIT_ON_EOF"):
    sys.exit(0)
# Force the adapter to exercise owned process-tree termination.
while not release.exists():
    time.sleep(0.05)
child.wait(timeout=3)
