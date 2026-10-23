#!/usr/bin/env python3
"""Opt-in bridge smoke; only creates/mutates its own synthetic localhost tab.

The read-only mode reports metadata shapes and tab counts, never page data.
No browser restart, profile copying, credentials, or system clipboard access.
"""

import argparse
import functools
import http.server
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import threading


class Bridge:
    def __init__(self, executable, selectors):
        self.proc = subprocess.Popen(
            [str(executable), "--mode", "mcp", *selectors],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8", bufsize=1,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        self.lines = queue.Queue()
        self.serial = 0
        threading.Thread(target=self._read, daemon=True).start()
        # Drain diagnostics without printing potentially private extension data.
        threading.Thread(target=lambda: self.proc.stderr.read(), daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def rpc(self, method, params):
        self.serial += 1
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.serial,
                                          "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()
        try:
            line = self.lines.get(timeout=35)
        except queue.Empty as exc:
            raise RuntimeError(f"{method} response timed out") from exc
        if line is None:
            raise RuntimeError("bridge exited before returning a response")
        response = json.loads(line)
        if response.get("id") != self.serial or "error" in response:
            raise RuntimeError(f"{method} returned a protocol error")
        return response["result"]

    def tool(self, name, arguments=None, allow_error=False):
        result = self.rpc("tools/call", {"name": name, "arguments": arguments or {}})
        if result.get("isError") and not allow_error:
            # Do not echo private response bodies into test logs.
            raise RuntimeError(f"{name} failed: " + str(result.get("content", []))[:1200])
        return result

    def value(self, name, arguments=None):
        result = self.tool(name, arguments)
        return json.loads(result["content"][0]["text"])

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge", type=Path, required=True)
    parser.add_argument("--browser-instance")
    parser.add_argument("--browser-family")
    parser.add_argument("--read-only", action="store_true")
    args = parser.parse_args()
    selectors = []
    for flag, value in [("--browser-instance", args.browser_instance),
                        ("--browser-family", args.browser_family)]:
        if value:
            selectors += [flag, value]
    bridge = Bridge(args.bridge.resolve(strict=True), selectors)
    tab_id = None
    server = None
    checks = []
    try:
        bridge.rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                  "clientInfo": {"name": "parity-smoke", "version": "1"}})
        info = bridge.value("codex_get_info")
        assert info["type"] == "extension"
        assert info["metadata"]["extensionInstanceId"]
        checks.append("getInfo-identity")
        tabs = bridge.value("codex_list_tabs")
        checks.append("listTabs")
        report = {"family": info["family"], "metadata_fields": sorted(info["metadata"]),
                  "owned_tab_count": len(tabs), "checks": checks}
        if args.read_only:
            print(json.dumps(report))
            return

        fixture_dir = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
        handler = functools.partial(QuietHandler, directory=str(fixture_dir))
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        created = bridge.tool("codex_create_tab")["content"][0]["text"]
        match = re.search(r"Created tab:\s*(\d+)", created)
        assert match, "createTab response shape changed"
        tab_id = match[1]
        bridge.tool("codex_nav_and_wait", {"tab_id": tab_id,
                    "url": f"http://127.0.0.1:{server.server_port}/locator.html", "timeout_ms": 15000})
        bridge.tool("codex_bring_to_front", {"tab_id": tab_id})
        bridge.tool("codex_cua_move", {"tab_id": tab_id, "x": 10, "y": 10})
        checks.append("cua-move")

        def locator(action, **fields):
            return bridge.value("codex_locator", {"tab_id": tab_id, "action": action, **fields})

        assert locator("count", role="button")["count"] >= 2
        ambiguous = bridge.tool("codex_locator", {"tab_id": tab_id, "action": "text", "role": "button"}, True)
        assert ambiguous.get("isError"), "strict ambiguous locator did not fail"
        locator("click", role="button", name="Save record", exact=True)
        assert locator("text", selector="#status")["value"] == "saved"
        locator("fill", label="Display name", value="Ada")
        assert locator("value", selector="#name")["value"] == "Ada"
        locator("set_checked", label="Enable feature", checked=True)
        assert locator("text", selector="#status")["value"] == "enabled"
        checks += ["locator-strict", "locator-click", "locator-fill", "locator-check"]

        def reset_status():
            bridge.tool("codex_evaluate", {
                "tab_id": tab_id,
                "expression": "document.querySelector('#status').textContent='idle'; true",
            })

        reset_status()
        bridge.tool("codex_click", {"tab_id": tab_id, "selector": "#primary"})
        assert locator("text", selector="#status")["value"] == "saved"
        checks.append("css-click-trusted-pointer")

        reset_status()
        bridge.tool("codex_hover", {"tab_id": tab_id, "selector": "#secondary"})
        assert locator("text", selector="#status")["value"] == "hovered"
        checks.append("css-hover-trusted-pointer")

        reset_status()
        locator("click", selector="#secondary", modifiers=["Control"])
        assert locator("text", selector="#status")["value"] == "ctrl-click"
        checks.append("locator-click-modifiers")

        reset_status()
        locator("click", selector="#secondary", button="right")
        assert locator("text", selector="#status")["value"] == "context"
        checks.append("locator-click-button")

        bridge.tool("codex_select_option", {
            "tab_id": tab_id, "selector": "#color", "value": "green",
        })
        assert locator("value", selector="#color")["value"] == "green"
        checks.append("select-option-exact")

        reset_status()
        bridge.tool("codex_cua_scroll", {
            "tab_id": tab_id, "x": 100, "y": 100, "scrollX": 0, "scrollY": 20,
        })
        assert locator("text", selector="#status")["value"] == "scrolled"
        checks.append("cua-scroll")

        reset_status()
        evaluated = bridge.tool("codex_evaluate", {
            "tab_id": tab_id,
            "expression": "JSON.stringify((()=>{const r=document.querySelector('#drag-pad').getBoundingClientRect();"
                          "return {x:r.left+r.width/2,y:r.top+r.height/2}})())",
        })
        point = json.loads(json.loads(evaluated["content"][0]["text"])["result"]["value"])
        bridge.tool("codex_drag", {
            "tab_id": tab_id,
            "path": [
                {"x": point["x"], "y": point["y"]},
                {"x": point["x"] + 30, "y": point["y"]},
                {"x": point["x"] + 60, "y": point["y"]},
            ],
        })
        assert locator("text", selector="#status")["value"] == "drag-end"
        checks.append("cua-drag-path")

        # The AX id is diagnostic; only backend node_id is actionable.
        matches = bridge.value("codex_find_element", {"tab_id": tab_id, "role": "button", "name": "Save record"})
        assert matches and matches[0]["actionable"] and matches[0]["node_id"]
        bridge.tool("codex_click_element", {"tab_id": tab_id, "node_id": matches[0]["node_id"]})
        assert locator("text", selector="#status")["value"] == "saved"
        checks.append("AX-backend-node-click")

        locator("press", selector="#name", keys=["Control", "a"])
        bridge.tool("codex_cua_type", {"tab_id": tab_id, "text": "Grace"})
        assert locator("value", selector="#name")["value"] == "Grace"
        checks.append("trusted-control-a")
        locator("wait", selector="#hidden-status", state="hidden", timeout_ms=1000)
        locator("wait", selector="#never-present", state="detached", timeout_ms=1000)
        failed_wait = bridge.tool("codex_locator", {"tab_id": tab_id, "action": "wait",
                          "selector": "button", "nth": 999, "timeout_ms": 100}, True)
        assert failed_wait.get("isError")
        checks.append("locator-wait-target-state")

        bridge.tool("codex_finalize")
        tab_id = None
        assert not bridge.value("codex_list_tabs")
        checks.append("turn-cleanup")
        print(json.dumps(report))
    finally:
        try:
            if tab_id:
                try:
                    bridge.tool("codex_close_tab", {"tab_id": tab_id})
                finally:
                    bridge.tool("codex_finalize")
        finally:
            bridge.close()
            if server:
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    main()
