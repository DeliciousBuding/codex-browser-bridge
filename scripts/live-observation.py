#!/usr/bin/env python3
"""Opt-in synthetic localhost acceptance for observation/locator/file contracts."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import re
import tempfile
import threading

from importlib.util import module_from_spec, spec_from_file_location

spec = spec_from_file_location("parity", Path(__file__).with_name("live-parity.py"))
parity = module_from_spec(spec)
spec.loader.exec_module(parity)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge", type=Path, required=True)
    parser.add_argument("--browser-instance", required=True)
    args = parser.parse_args()
    fixture = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(parity.QuietHandler, directory=str(fixture)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    bridge = parity.Bridge(args.bridge.resolve(strict=True), ["--browser-instance", args.browser_instance])
    tab = None
    checks = []
    try:
        bridge.rpc("initialize", {"protocolVersion":"2025-11-25", "capabilities":{}, "clientInfo":{"name":"observation-smoke","version":"1"}})
        created = bridge.tool("codex_create_tab")["content"][0]["text"]
        tab = re.search(r"Created tab:\s*(\d+)", created)[1]
        observe = lambda action, **fields: bridge.value("codex_observe", {"tab_id":tab,"action":action,**fields})
        locator = lambda action, **fields: bridge.value("codex_locator", {"tab_id":tab,"action":action,**fields})
        armed = observe("start")
        url = f"http://127.0.0.1:{server.server_port}/observation.html"
        bridge.tool("codex_nav_and_wait", {"tab_id":tab,"url":url,"timeout_ms":15000})
        state = observe("state")
        assert state["document_epoch"] > armed["document_epoch"] and state["load"]
        assert state["url"] == url and state["frames"]
        checks.append("document-epoch-frame-navigation")
        logs = observe("logs", cursor=armed["cursor"])
        assert any("early-fixture-log" in log["text"] for log in logs["logs"])
        checks.append("early-log-persistent-cursor")
        assert locator("visible", selector="#absent")["value"] is False
        assert locator("count", selector="button", **{"or":[{"selector":"#next"}]})["count"] >= 2
        values = locator("all", selector="option")["values"]
        assert len(values) == 2
        locator("select_option", selector="#multi", options=[{"label":"Red"},{"index":1}])
        selected = bridge.value("codex_evaluate", {"tab_id":tab,"expression":"Array.from(document.querySelector('#multi').selectedOptions).map(o=>o.value)"})
        assert selected["result"]["value"] == ["red","green"]
        checks.append("locator-boolean-composition-list-select")
        locator("click", selector="#alert")
        dialog = bridge.value("codex_dialog", {"tab_id":tab,"action":"get"})["dialog"]
        assert dialog["dialog_type"] == "alert" and dialog["message"] == "fixture dialog"
        bridge.tool("codex_dialog", {"tab_id":tab,"action":"dismiss","dialog_id":dialog["id"]})
        stale = bridge.tool("codex_dialog", {"tab_id":tab,"action":"dismiss","dialog_id":dialog["id"]}, True)
        assert stale.get("isError")
        checks.append("dialog-state-stale-rejection")
        bridge.tool("codex_file_chooser", {"tab_id":tab,"action":"start"})
        locator("click", selector="#upload")
        chooser = bridge.value("codex_file_chooser", {"tab_id":tab,"action":"state"})["chooser"]
        assert chooser and chooser["mode"] == "selectMultiple"
        denied = bridge.tool("codex_file_chooser", {"tab_id":tab,"action":"set_files","chooser_id":chooser["id"],"files":["/not-authorized"]}, True)
        assert denied.get("isError")
        bridge.tool("codex_file_chooser", {"tab_id":tab,"action":"stop"})
        checks.append("file-chooser-mode-authority-cleanup")
        before_nav = observe("state")
        bridge.tool("codex_click_and_wait", {"tab_id":tab,"selector":"#next","timeout_ms":15000})
        after_nav = observe("state")
        assert after_nav["document_epoch"] > before_nav["document_epoch"] and "from=observation" in after_nav["url"]
        observed_nav = observe("wait_navigation", cursor=before_nav["cursor"], timeout_ms=1000)
        assert observed_nav["load"]
        checks.append("click-navigation-armed-before-action")
        locator("fill", selector="#name", value="Prefix")
        locator("press_sequentially", selector="#name", value="-汉字", delay_ms=1)
        typed = locator("value", selector="#name")["value"]
        assert typed == "Prefix-汉字", repr(typed)
        checks.append("unicode-sequential-append")
        with tempfile.TemporaryDirectory(prefix="bridge-file-smoke-") as output:
            inventory = bridge.value("codex_page_assets", {"tab_id":tab,"action":"list"})
            # Empty selection still emits a bounded manifest through one atomic bundle.
            bundle = bridge.value("codex_page_assets", {"tab_id":tab,"action":"bundle","inventory_id":inventory["id"],"output_dir":output,"asset_ids":[]})
            assert bundle["path_scope"] == "bridge_host" and bundle["browser_download"] is False
            checks.append("atomic-file-result-path-scope")
        bridge.tool("codex_finalize")
        tab = None
        print(json.dumps({"checks":checks}))
    finally:
        if tab:
            try:
                bridge.tool("codex_finalize")
            except Exception:
                pass
        bridge.close()
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
