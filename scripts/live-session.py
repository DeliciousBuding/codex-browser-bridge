#!/usr/bin/env python3
"""Opt-in multi-process/ownership smoke; creates only disposable agent tabs."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

spec = importlib.util.spec_from_file_location("parity", Path(__file__).with_name("live-parity.py"))
parity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(parity)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge", type=Path, required=True)
    parser.add_argument("--browser-instance", required=True)
    args = parser.parse_args()
    executable = args.bridge.resolve(strict=True)
    selectors = ["--browser-instance", args.browser_instance]
    checks = []
    with tempfile.TemporaryDirectory(prefix="bridge-session-smoke-") as temporary:
        path = Path(temporary) / "session.json"
        def call(name, arguments=None, instance=args.browser_instance, allow_error=False):
            result = subprocess.run([str(executable), "--browser-instance", instance,
                "--session-file", str(path), "call", name, json.dumps(arguments or {})],
                capture_output=True, text=True, encoding="utf-8", timeout=40,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            if allow_error:
                assert result.returncode != 0, "expected selector conflict"
                return None
            assert result.returncode == 0, f"{name} failed (private response omitted)"
            return json.loads(result.stdout)

        try:
            created = call("codex_create_tab")
            tab = created["content"][0]["text"].split(":", 1)[1].strip()
            saved = json.loads(path.read_text(encoding="utf-8"))
            assert saved["version"] == 2 and saved["browser"]["instance_id"] == args.browser_instance
            owned = json.loads(call("codex_list_tabs")["content"][0]["text"])
            assert any(str(item["id"]) == tab for item in owned)
            call("codex_get_info", instance="synthetic-wrong-instance", allow_error=True)
            assert json.loads(path.read_text(encoding="utf-8"))["turn_id"] == saved["turn_id"]
            checks += ["process-continuation", "exact-instance-conflict-preserves-token"]
            call("codex_finalize")
            assert not path.exists()
            checks.append("finalize-removes-token")
        finally:
            if path.exists():
                call("codex_finalize")

    clients = [parity.Bridge(executable, selectors), parity.Bridge(executable, selectors)]
    owned = []
    try:
        for client in clients:
            client.tool("codex_create_tab")
            owned.append(client.value("codex_list_tabs"))
        ids = [{str(tab["id"]) for tab in tabs} for tabs in owned]
        assert ids[0] and ids[1] and ids[0].isdisjoint(ids[1]), "separate sessions must not see each other's owned tabs"
        checks.append("two-client-owned-tab-isolation")
        # Explicitly attempt only a synthetic neighbor agent tab. No user tabs.
        neighbor = next(iter(ids[1]))
        denied = clients[0].tool("codex_get_title", {"tab_id": neighbor}, True)
        assert denied.get("isError"), "foreign synthetic tab unexpectedly accessible"
        checks.append("foreign-session-tab-denied")
        clients[0].tool("codex_finalize")
        remaining = {str(tab["id"]) for tab in clients[1].value("codex_list_tabs")}
        assert remaining == ids[1], "one client's finalize changed another client's tabs"
        checks.append("finalize-preserves-other-client")
        print(json.dumps({"checks": checks}))
    finally:
        for client in clients:
            try:
                client.tool("codex_finalize")
            finally:
                client.close()


if __name__ == "__main__":
    main()
