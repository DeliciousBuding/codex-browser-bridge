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
(root / "environment.json").write_text(
    json.dumps(
        {
            name: os.environ.get(name)
            for name in [
                "BRIDGE_NATIVE_FIXTURE_FORWARDED",
                "BRIDGE_NATIVE_FIXTURE_LOCAL_FORWARDED",
                "BRIDGE_NATIVE_FIXTURE_LITERAL",
                "DISPLAY",
                "XAUTHORITY",
                "WAYLAND_DISPLAY",
                "XDG_RUNTIME_DIR",
                "DBUS_SESSION_BUS_ADDRESS",
                "CUA_REPL_ENABLED_SURFACES",
                "NODE_REPL_TRUSTED_SERVICES",
                "SSL_CERT_FILE",
                "BRIDGE_NATIVE_FIXTURE_AMBIENT",
                "PATH",
                "PATHEXT",
                "SYSTEMROOT",
                "NODE_REPL_AUTH_TOKEN",
            ]
        }
    )
)
end_count = 0
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
            "protocolVersion": os.environ.get(
                "BRIDGE_NATIVE_FIXTURE_PROTOCOL", row["params"]["protocolVersion"]
            ),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "synthetic-native", "version": "1"},
        }
        if os.environ.get("BRIDGE_NATIVE_FIXTURE_PROTOCOL") == "<missing>":
            result.pop("protocolVersion")
    elif row["method"] == "tools/list":
        if os.environ.get("BRIDGE_NATIVE_FIXTURE_HANG_CATALOG"):
            continue
        if os.environ.get("BRIDGE_NATIVE_FIXTURE_LOOP_CATALOG"):
            result = {
                "tools": [{"name": "js", "inputSchema": {"type": "object"}}],
                "nextCursor": "loop",
            }
        elif os.environ.get("BRIDGE_NATIVE_FIXTURE_PAGINATE_CATALOG"):
            if row.get("params", {}).get("cursor") is None:
                result = {
                    "tools": [
                        {"name": "js", "inputSchema": {"type": "object"}},
                        {"name": "js_reset", "inputSchema": {"type": "object"}},
                    ],
                    "nextCursor": "page-2",
                }
            elif row["params"]["cursor"] == "page-2":
                result = {
                    "tools": [
                        {
                            "name": "js_add_node_module_dir",
                            "inputSchema": {"type": "object"},
                        },
                        {"name": "turn_ended", "inputSchema": {"type": "object"}},
                    ]
                }
            else:
                result = {"tools": []}
        else:
            result = {
                "tools": [
                    {"name": "js", "inputSchema": {"type": "object"}},
                    {"name": "js_reset", "inputSchema": {"type": "object"}},
                    {
                        "name": "js_add_node_module_dir",
                        "inputSchema": {"type": "object"},
                    },
                    {"name": "turn_ended", "inputSchema": {"type": "object"}},
                ]
            }
    else:
        if row.get("params", {}).get("name") == "turn_ended":
            end_count += 1
            end_error = os.environ.get("BRIDGE_NATIVE_FIXTURE_END_ERROR")
            if end_error and end_count == 1:
                reply = {"jsonrpc": "2.0", "id": row["id"]}
                if end_error == "rpc":
                    reply["error"] = {"code": -32603, "message": "synthetic hook rejected", "data": {"keep": True}}
                else:
                    reply["result"] = {"isError": True, "content": []}
                print(json.dumps(reply), flush=True)
                continue
        if row.get("params", {}).get("name") == "js" and os.environ.get(
            "BRIDGE_NATIVE_FIXTURE_RPC_ERROR"
        ):
            print(json.dumps({
                "jsonrpc": "2.0", "id": row["id"],
                "error": {"code": -32603, "message": "synthetic\nRPC error",
                          "data": {"native": ["line 1\n\tline 2", {"keep": True}]}},
                "_meta": {"nativeError": True}
            }), flush=True)
            continue
        if row.get("params", {}).get("name") == "turn_ended" and os.environ.get(
            "BRIDGE_NATIVE_FIXTURE_HANG_CLEANUP"
        ):
            continue
        if row.get("params", {}).get("name") == "js" and os.environ.get(
            "BRIDGE_NATIVE_FIXTURE_HANG_JS"
        ):
            continue
        result = {"content": [{"type": "text", "text": "synthetic response"}]}
        if row.get("params", {}).get("name") == "js" and os.environ.get(
            "BRIDGE_NATIVE_FIXTURE_IMAGE_RESULT"
        ):
            result["content"] = [
                {
                    "type": "image",
                    "mimeType": "image/png",
                    "data": "aGVsbG8=",
                    "_meta": {"screenshotId": "synthetic-image"},
                }
            ]
        if row.get("params", {}).get("name") == "js" and os.environ.get(
            "BRIDGE_NATIVE_FIXTURE_JS_ERROR"
        ):
            result["isError"] = True
    print(json.dumps({"jsonrpc": "2.0", "id": row["id"], "result": result}), flush=True)
(root / "stdin-eof").write_text(str(os.getpid()))
if not os.environ.get("BRIDGE_NATIVE_FIXTURE_HOLD_ON_EOF"):
    sys.exit(0)
# Force the adapter to exercise owned process-tree termination after grace timeout.
while not release.exists():
    time.sleep(0.05)
child.wait(timeout=3)
