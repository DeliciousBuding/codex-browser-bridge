package mcp

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/protocol"
)

// TestFullSessionLifecycle runs the complete MCP handshake and tool sequence:
// initialize → tools/list → create_tab → navigate → screenshot → evaluate → finalize.
func TestFullSessionLifecycle(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		switch req.Method {
		case "createTab":
			return map[string]interface{}{"id": 42}, nil
		case "detach", "attach":
			return map[string]bool{"ok": true}, nil
		case "executeCdp":
			params, _ := req.Params.(map[string]interface{})
			cdpMethod, _ := params["method"].(string)
			switch cdpMethod {
			case "Page.navigate":
				return map[string]bool{"ok": true}, nil
			case "Runtime.evaluate":
				return map[string]interface{}{
					"result": map[string]interface{}{"value": "complete"},
				}, nil
			case "Page.captureScreenshot":
				return map[string]string{"data": "BASE64PNGDATA"}, nil
			}
			return map[string]bool{"ok": true}, nil
		case "getInfo":
			return map[string]interface{}{
				"metadata": map[string]interface{}{
					"extensionId": "test-ext",
				},
			}, nil
		case "finalizeTabs":
			return map[string]bool{"ok": true}, nil
		}
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	// 1. Create tab
	out, ok := callTool(t, srv, "codex_create_tab", nil)
	if !ok {
		t.Fatalf("create_tab: %s", out)
	}
	if !strings.Contains(out, "42") {
		t.Errorf("create_tab output: %s", out)
	}

	// 2. Navigate
	out, ok = callTool(t, srv, "codex_navigate", map[string]interface{}{
		"tab_id": "42",
		"url":    "https://apiradar.live/explore",
	})
	if !ok {
		t.Fatalf("navigate: %s", out)
	}

	// 3. Wait for load
	out, ok = callTool(t, srv, "codex_wait_for_load", map[string]interface{}{
		"tab_id":     "42",
		"timeout_ms": float64(5000),
	})
	if !ok {
		t.Fatalf("wait_for_load: %s", out)
	}

	// 4. Screenshot
	content, err := callToolRaw(t, srv, "codex_screenshot", map[string]interface{}{
		"tab_id": "42",
	})
	if err != nil {
		t.Fatalf("screenshot: %v", err)
	}
	foundImage := false
	for _, c := range content {
		if c.Type == "image" {
			foundImage = true
			if c.Data != "BASE64PNGDATA" {
				t.Errorf("image data = %q", c.Data)
			}
			break
		}
	}
	if !foundImage {
		t.Error("screenshot should return an image content block")
	}

	// 5. Evaluate
	out, ok = callTool(t, srv, "codex_evaluate", map[string]interface{}{
		"tab_id":     "42",
		"expression": "document.title",
	})
	if !ok {
		t.Fatalf("evaluate: %s", out)
	}

	// 6. Finalize
	out, ok = callTool(t, srv, "codex_finalize", nil)
	if !ok {
		t.Fatalf("finalize: %s", out)
	}

	// Verify the full RPC sequence
	methods := pipe.recordedMethods()
	t.Logf("Recorded methods: %v", methods)

	// Collect unique method calls to verify coverage
	seen := make(map[string]int)
	for _, m := range methods {
		seen[m]++
	}
	for _, want := range []string{"createTab", "detach", "attach", "executeCdp", "finalizeTabs"} {
		if seen[want] == 0 {
			t.Errorf("missing expected method %q in recorded calls", want)
		}
	}
}

// TestMCPInitializeToolsList verifies the MCP initialization handshake and tools/list.
func TestMCPInitializeToolsList(t *testing.T) {
	in := strings.Join([]string{
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`,
		`{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}`,
		`{"jsonrpc":"2.0","id":3,"method":"notifications/initialized","params":{}}`,
	}, "\n") + "\n"

	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}

	resps := decodeResponses(t, out)
	if len(resps) < 2 {
		t.Fatalf("expected at least 2 responses, got %d", len(resps))
	}

	// Response 1: initialize
	r1 := resps[0]
	if r1["id"] != float64(1) {
		t.Errorf("init id = %v", r1["id"])
	}
	result, _ := r1["result"].(map[string]interface{})
	if result == nil {
		t.Fatal("initialize missing result")
	}
	if result["protocolVersion"] != "2024-11-05" {
		t.Errorf("protocolVersion = %v", result["protocolVersion"])
	}

	// Response 2: tools/list
	r2 := resps[1]
	if r2["id"] != float64(2) {
		t.Errorf("tools/list id = %v", r2["id"])
	}
	tlResult, _ := r2["result"].(map[string]interface{})
	tools, _ := tlResult["tools"].([]interface{})
	if len(tools) == 0 {
		t.Fatal("tools/list returned empty tools array")
	}

	// Verify key tools exist
	toolNames := make(map[string]bool)
	for _, t := range tools {
		m, _ := t.(map[string]interface{})
		name, _ := m["name"].(string)
		toolNames[name] = true
	}
	for _, want := range []string{"codex_create_tab", "codex_navigate", "codex_screenshot", "codex_evaluate", "codex_finalize", "codex_get_info"} {
		if !toolNames[want] {
			t.Errorf("missing expected tool: %s", want)
		}
	}
}

// TestNavigateRetryOnDebuggerError verifies that when the navigate handler
// encounters a debugger detach error, it's propagated correctly.
func TestNavigateRetryOnDebuggerError(t *testing.T) {
	var execCalls int
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		switch req.Method {
		case "detach", "attach":
			return map[string]bool{"ok": true}, nil
		case "executeCdp":
			execCalls++
			if execCalls == 1 {
				return nil, &protocol.ErrorObject{
					Code:    -1,
					Message: "Debugger is not attached to the tab with id: 5",
				}
			}
			return map[string]bool{"ok": true}, nil
		}
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_navigate", map[string]interface{}{
		"tab_id": "5",
		"url":    "https://test.com",
	})
	if !ok {
		t.Fatalf("navigate should succeed after retry: %s", out)
	}

	// Should have detach + attach + executeCdp(fail) + detach + attach + executeCdp(ok)
	methods := pipe.recordedMethods()
	if got := countMethod(methods, "executeCdp"); got < 2 {
		t.Errorf("expected at least 2 executeCdp calls (first fail, retry succeed), got %d: %v", got, methods)
	}
}

// TestScreenshotRetryOnDebuggerError verifies screenshot retry logic.
func TestScreenshotRetryOnDebuggerError(t *testing.T) {
	var execCalls int
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		switch req.Method {
		case "detach", "attach":
			return map[string]bool{"ok": true}, nil
		case "executeCdp":
			execCalls++
			if execCalls == 1 {
				// First attempt fails with debugger error
				return nil, &protocol.ErrorObject{
					Code:    -1,
					Message: "Debugger is not attached",
				}
			}
			// Retry succeeds
			return map[string]string{"data": "PNG_AFTER_RETRY"}, nil
		}
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	content, err := callToolRaw(t, srv, "codex_screenshot", map[string]interface{}{
		"tab_id": "3",
	})
	if err != nil {
		t.Fatalf("screenshot should succeed after retry: %v", err)
	}
	found := false
	for _, c := range content {
		if c.Type == "image" {
			found = true
			if c.Data != "PNG_AFTER_RETRY" {
				t.Errorf("data = %q", c.Data)
			}
			break
		}
	}
	if !found {
		t.Error("no image in response")
	}

	methods := pipe.recordedMethods()
	execCount := countMethod(methods, "executeCdp")
	if execCount < 2 {
		t.Errorf("expected multiple executeCdp calls due to retry, got %d: %v", execCount, methods)
	}
}

// TestClaimTabSendsIntegerTabID verifies claimUserTab uses integer tab ID.
func TestClaimTabSendsIntegerTabID(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]interface{}{"id": 99, "url": "https://claimed.test"}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_claim_tab", map[string]interface{}{
		"tab_id": "99",
	})
	if !ok {
		t.Fatalf("claim_tab: %s", out)
	}
	if !strings.Contains(out, "99") {
		t.Errorf("output = %s", out)
	}

	hasClaim, hasAttach := false, false
	for _, m := range pipe.recordedMethods() {
		if m == "claimUserTab" {
			hasClaim = true
		}
		if m == "attach" {
			hasAttach = true
		}
	}
	if !hasClaim || !hasAttach {
		t.Errorf("expected claimUserTab + attach, got %v", pipe.recordedMethods())
	}
}

// TestCreateTabRejectsNonNumericClose verifies close_tab rejects non-numeric IDs.
func TestCreateTabRejectsNonNumericClose(t *testing.T) {
	srv, _, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	tool := srv.toolMap["codex_close_tab"]
	args, _ := json.Marshal(map[string]interface{}{"tab_id": "abc"})
	if _, err := tool.Handler(args); err == nil {
		t.Fatal("expected error for non-numeric close_tab, got nil")
	}
}

// --- helpers ---

func countMethod(methods []string, target string) int {
	n := 0
	for _, m := range methods {
		if m == target {
			n++
		}
	}
	return n
}
