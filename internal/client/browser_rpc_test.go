package client

import (
	"encoding/json"
	"strings"
	"sync"
	"testing"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/protocol"
)

type recordedCall struct {
	method string
	params map[string]interface{}
}

type recorder struct {
	mu    sync.Mutex
	calls []recordedCall
}

func (r *recorder) record(req protocol.Request) {
	r.mu.Lock()
	defer r.mu.Unlock()
	p, _ := req.Params.(map[string]interface{})
	r.calls = append(r.calls, recordedCall{method: req.Method, params: p})
}

func (r *recorder) snapshot() []recordedCall {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make([]recordedCall, len(r.calls))
	copy(out, r.calls)
	return out
}

// withRecordingServer makes a client+server where every request is recorded
// and the handler decides how to reply.
func withRecordingServer(t *testing.T, reply func(req protocol.Request) interface{}) (*Client, *recorder, func()) {
	t.Helper()
	rec := &recorder{}
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		rec.record(req)
		return reply(req), nil
	})
	cleanup := func() { srv.close(); c.Close() }
	return c, rec, cleanup
}

func TestCloseTabRejectsNonNumericID(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()
	if err := c.CloseTab("not-a-number"); err == nil {
		t.Fatal("expected error for non-numeric id, got nil")
	}
}

func TestNavigateRejectsNonNumericID(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()
	if err := c.Navigate("oops", "https://example.com"); err == nil {
		t.Fatal("expected error for non-numeric id, got nil")
	}
}

func TestNavigateSendsCDPNavigate(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.Navigate("17", "https://example.com"); err != nil {
		t.Fatalf("Navigate: %v", err)
	}

	calls := rec.snapshot()
	if len(calls) < 2 {
		t.Fatalf("expected attach + executeCdp, got %d calls", len(calls))
	}
	if calls[0].method != "attach" {
		t.Errorf("first call %q, want attach", calls[0].method)
	}
	if calls[1].method != "executeCdp" {
		t.Errorf("second call %q, want executeCdp", calls[1].method)
	}
	target, _ := calls[1].params["target"].(map[string]interface{})
	if target == nil {
		t.Fatalf("executeCdp missing nested target: %+v", calls[1].params)
	}
	if got, ok := target["tabId"].(float64); !ok || int(got) != 17 {
		t.Errorf("target.tabId = %v, want 17", target["tabId"])
	}
	if calls[1].params["method"] != "Page.navigate" {
		t.Errorf("CDP method = %v, want Page.navigate", calls[1].params["method"])
	}
	cmd, _ := calls[1].params["commandParams"].(map[string]interface{})
	if cmd["url"] != "https://example.com" {
		t.Errorf("commandParams.url = %v", cmd["url"])
	}
}

func TestListTabsDecodesBareArray(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return []map[string]interface{}{
			{"id": 5, "url": "https://a.test", "title": "A"},
			{"id": 6, "url": "https://b.test", "title": "B"},
		}
	})
	defer cleanup()

	tabs, err := c.ListTabs()
	if err != nil {
		t.Fatalf("ListTabs: %v", err)
	}
	if len(tabs) != 2 {
		t.Fatalf("len = %d, want 2", len(tabs))
	}
	if tabs[0].ID != "5" || tabs[1].ID != "6" {
		t.Errorf("ids = %q, %q", tabs[0].ID, tabs[1].ID)
	}
}

func TestListUserTabsAcceptsWrappedAndBare(t *testing.T) {
	tests := []struct {
		name  string
		reply interface{}
	}{
		{"wrapped", map[string]interface{}{
			"tabs": []map[string]interface{}{{"id": 1, "url": "x", "title": "y"}},
		}},
		{"bare", []map[string]interface{}{{"id": 1, "url": "x", "title": "y"}}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return tt.reply })
			defer cleanup()
			tabs, err := c.ListUserTabs()
			if err != nil {
				t.Fatalf("ListUserTabs: %v", err)
			}
			if len(tabs) != 1 || tabs[0].ID != "1" {
				t.Errorf("got %+v", tabs)
			}
		})
	}
}

func TestCreateTabReturnsID(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return map[string]interface{}{"id": 42, "url": "about:blank"}
	})
	defer cleanup()

	id, err := c.CreateTab()
	if err != nil {
		t.Fatalf("CreateTab: %v", err)
	}
	if id != "42" {
		t.Errorf("id = %q, want 42", id)
	}
	if calls := rec.snapshot(); len(calls) != 1 || calls[0].method != "createTab" {
		t.Errorf("calls = %+v", calls)
	}
}

func TestClaimUserTabSendsIntegerTabID(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return map[string]interface{}{"id": 7, "url": "https://example.com"}
	})
	defer cleanup()

	tab, err := c.ClaimUserTab("7")
	if err != nil {
		t.Fatalf("ClaimUserTab: %v", err)
	}
	if tab.ID != "7" {
		t.Errorf("returned tab id = %q", tab.ID)
	}

	calls := rec.snapshot()
	if len(calls) < 1 || calls[0].method != "claimUserTab" {
		t.Fatalf("first call %+v, want claimUserTab", calls)
	}
	got, ok := calls[0].params["tabId"].(float64)
	if !ok {
		t.Fatalf("tabId is %T, want number (Chrome extension API expects int)", calls[0].params["tabId"])
	}
	if int(got) != 7 {
		t.Errorf("tabId = %v, want 7", got)
	}

	hasAttach := false
	for _, c := range calls {
		if c.method == "attach" {
			hasAttach = true
			break
		}
	}
	if !hasAttach {
		t.Errorf("expected auto-attach after claim, calls = %+v", calls)
	}
}

func TestClaimUserTabRejectsNonNumeric(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{} })
	defer cleanup()
	if _, err := c.ClaimUserTab("abc"); err == nil {
		t.Fatal("expected error for non-numeric id, got nil")
	}
}

func TestScreenshotExtractsBase64Data(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method == "executeCdp" {
			return map[string]string{"data": "iVBORw0KGgo..."}
		}
		return map[string]bool{"ok": true}
	})
	defer cleanup()

	b64, err := c.Screenshot("3", false)
	if err != nil {
		t.Fatalf("Screenshot: %v", err)
	}
	if b64 != "iVBORw0KGgo..." {
		t.Errorf("expected base64 data extracted, got %q", b64)
	}
}

func TestWaitForLoadReturnsOnComplete(t *testing.T) {
	var calls int
	var mu sync.Mutex
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}
		}
		mu.Lock()
		calls++
		state := "loading"
		if calls >= 2 {
			state = "complete"
		}
		mu.Unlock()
		return map[string]interface{}{"result": map[string]interface{}{"value": state}}
	})
	defer cleanup()

	state, err := c.WaitForLoad("3", 5000)
	if err != nil {
		t.Fatalf("WaitForLoad: %v", err)
	}
	if state != "complete" {
		t.Errorf("final state = %q, want complete", state)
	}
}

func TestWaitForLoadTimesOut(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}
		}
		return map[string]interface{}{"result": map[string]interface{}{"value": "loading"}}
	})
	defer cleanup()

	state, err := c.WaitForLoad("3", 250)
	if err == nil {
		t.Fatal("expected timeout error, got nil")
	}
	if state != "loading" {
		t.Errorf("last observed state = %q, want loading", state)
	}
}

func TestWaitForLoadRejectsNonNumeric(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{} })
	defer cleanup()
	if _, err := c.WaitForLoad("abc", 1000); err == nil {
		t.Fatal("expected error for non-numeric tab id, got nil")
	}
}

func TestEvaluateForwardsExpression(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return map[string]interface{}{"result": map[string]interface{}{"value": 99}}
	})
	defer cleanup()

	raw, err := c.Evaluate("1", "1+2")
	if err != nil {
		t.Fatalf("Evaluate: %v", err)
	}
	if !strings.Contains(string(raw), `"value":99`) {
		t.Errorf("raw result = %s", raw)
	}

	for _, call := range rec.snapshot() {
		if call.method != "executeCdp" {
			continue
		}
		cmd, _ := call.params["commandParams"].(map[string]interface{})
		if cmd["expression"] != "1+2" {
			t.Errorf("expression forwarded as %v", cmd["expression"])
		}
		if cmd["returnByValue"] != true {
			t.Errorf("returnByValue = %v, want true", cmd["returnByValue"])
		}
	}
}

func TestGetInfoReturnsRawResult(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return map[string]string{"version": "1.2.3"}
	})
	defer cleanup()

	raw, err := c.GetInfo()
	if err != nil {
		t.Fatalf("GetInfo: %v", err)
	}
	var got map[string]string
	if err := json.Unmarshal(raw, &got); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if got["version"] != "1.2.3" {
		t.Errorf("version = %v", got["version"])
	}
}

func TestReloadSendsCDPReload(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.Reload("3"); err != nil {
		t.Fatalf("Reload: %v", err)
	}
	for _, call := range rec.snapshot() {
		if call.method == "executeCdp" && call.params["method"] != "Page.reload" {
			t.Errorf("CDP method = %v, want Page.reload", call.params["method"])
		}
	}
}

func TestNameSessionSendsParam(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.NameSession("my-session"); err != nil {
		t.Fatalf("NameSession: %v", err)
	}
	calls := rec.snapshot()
	if len(calls) != 1 || calls[0].method != "nameSession" {
		t.Fatalf("calls = %+v", calls)
	}
	if calls[0].params["name"] != "my-session" {
		t.Errorf("name param = %v", calls[0].params["name"])
	}
}

func TestFinalizeTabsWithKeep(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	keep := []map[string]interface{}{{"id": 5}}
	if err := c.FinalizeTabs(keep); err != nil {
		t.Fatalf("FinalizeTabs: %v", err)
	}
	calls := rec.snapshot()
	if len(calls) != 1 || calls[0].method != "finalizeTabs" {
		t.Fatalf("calls = %+v", calls)
	}
	if _, ok := calls[0].params["keep"]; !ok {
		t.Errorf("keep param missing: %+v", calls[0].params)
	}
}

func TestFinalizeTabsNilKeep(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.FinalizeTabs(nil); err != nil {
		t.Fatalf("FinalizeTabs: %v", err)
	}
	calls := rec.snapshot()
	if _, ok := calls[0].params["keep"]; ok {
		t.Errorf("keep should be omitted when nil, got: %+v", calls[0].params)
	}
}

func TestNavigateBackUsesHistoryEntry(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}
		}
		params, _ := req.Params.(map[string]interface{})
		if params["method"] == "Page.getNavigationHistory" {
			return map[string]interface{}{
				"currentIndex": 2,
				"entries": []map[string]interface{}{
					{"id": 100, "url": "https://a.test"},
					{"id": 101, "url": "https://b.test"},
					{"id": 102, "url": "https://c.test"},
				},
			}
		}
		return map[string]bool{"ok": true}
	})
	defer cleanup()

	if err := c.NavigateBack("3"); err != nil {
		t.Fatalf("NavigateBack: %v", err)
	}

	var navCall *recordedCall
	for i, call := range rec.snapshot() {
		if call.method == "executeCdp" && call.params["method"] == "Page.navigateToHistoryEntry" {
			c := rec.snapshot()[i]
			navCall = &c
			break
		}
	}
	if navCall == nil {
		t.Fatal("Page.navigateToHistoryEntry not called")
	}
	cmd, _ := navCall.params["commandParams"].(map[string]interface{})
	if int(cmd["entryId"].(float64)) != 101 {
		t.Errorf("entryId = %v, want 101 (previous entry)", cmd["entryId"])
	}
}

func TestNavigateBackErrorsAtHistoryStart(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}
		}
		params, _ := req.Params.(map[string]interface{})
		if params["method"] == "Page.getNavigationHistory" {
			return map[string]interface{}{
				"currentIndex": 0,
				"entries":      []map[string]interface{}{{"id": 100, "url": "https://a.test"}},
			}
		}
		return map[string]bool{"ok": true}
	})
	defer cleanup()

	if err := c.NavigateBack("3"); err == nil {
		t.Fatal("expected error at history start, got nil")
	}
}

func TestNavigateForwardErrorsAtHistoryEnd(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}
		}
		params, _ := req.Params.(map[string]interface{})
		if params["method"] == "Page.getNavigationHistory" {
			return map[string]interface{}{
				"currentIndex": 1,
				"entries": []map[string]interface{}{
					{"id": 100, "url": "https://a.test"},
					{"id": 101, "url": "https://b.test"},
				},
			}
		}
		return map[string]bool{"ok": true}
	})
	defer cleanup()

	if err := c.NavigateForward("3"); err == nil {
		t.Fatal("expected error at history end, got nil")
	}
}

func TestClickEscapesSelector(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	selector := `button[data-id="x\"y"]`
	if err := c.Click("3", selector); err != nil {
		t.Fatalf("Click: %v", err)
	}

	var expr string
	for _, call := range rec.snapshot() {
		if call.method == "executeCdp" {
			cmd, _ := call.params["commandParams"].(map[string]interface{})
			expr, _ = cmd["expression"].(string)
		}
	}
	if expr == "" {
		t.Fatal("no expression sent")
	}
	if !strings.Contains(expr, "querySelector") || !strings.Contains(expr, ".click()") {
		t.Errorf("expression doesn't look like a click: %s", expr)
	}
	if strings.Contains(expr, "x\"y") {
		t.Errorf("selector quotes not escaped — would break JS: %s", expr)
	}
}

func TestFillEscapesValue(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	value := `pwd"with\quote`
	if err := c.Fill("3", "input#x", value); err != nil {
		t.Fatalf("Fill: %v", err)
	}

	var expr string
	for _, call := range rec.snapshot() {
		if call.method == "executeCdp" {
			cmd, _ := call.params["commandParams"].(map[string]interface{})
			expr, _ = cmd["expression"].(string)
		}
	}
	if !strings.Contains(expr, "el.focus()") || !strings.Contains(expr, "el.value") {
		t.Errorf("expression doesn't look like a fill: %s", expr)
	}
	if strings.Contains(expr, `pwd"with`) {
		t.Errorf("value quotes not escaped: %s", expr)
	}
}

func TestCUAClickSendsPressAndRelease(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.CUAClick("3", 100, 200); err != nil {
		t.Fatalf("CUAClick: %v", err)
	}

	types := []string{}
	for _, call := range rec.snapshot() {
		if call.method == "executeCdp" {
			cmd, _ := call.params["commandParams"].(map[string]interface{})
			if t, ok := cmd["type"].(string); ok {
				types = append(types, t)
			}
		}
	}
	if len(types) != 2 || types[0] != "mousePressed" || types[1] != "mouseReleased" {
		t.Errorf("expected mousePressed then mouseReleased, got %v", types)
	}
}

func TestCUATypeSendsCharPerRune(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.CUAType("3", "abc"); err != nil {
		t.Fatalf("CUAType: %v", err)
	}

	chars := []string{}
	for _, call := range rec.snapshot() {
		if call.method == "executeCdp" {
			cmd, _ := call.params["commandParams"].(map[string]interface{})
			if cmd["type"] == "char" {
				chars = append(chars, cmd["text"].(string))
			}
		}
	}
	if strings.Join(chars, "") != "abc" {
		t.Errorf("typed chars = %v, want a,b,c", chars)
	}
}

func TestCUAKeypressSendsDownThenUp(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.CUAKeypress("3", []string{"Enter"}); err != nil {
		t.Fatalf("CUAKeypress: %v", err)
	}

	types := []string{}
	for _, call := range rec.snapshot() {
		if call.method == "executeCdp" {
			cmd, _ := call.params["commandParams"].(map[string]interface{})
			if t, ok := cmd["type"].(string); ok {
				types = append(types, t)
			}
		}
	}
	if len(types) != 2 || types[0] != "keyDown" || types[1] != "keyUp" {
		t.Errorf("expected keyDown then keyUp, got %v", types)
	}
}

func TestCUAScrollSendsWheelDelta(t *testing.T) {
	c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()

	if err := c.CUAScroll("3", 10, 20, 0, 100); err != nil {
		t.Fatalf("CUAScroll: %v", err)
	}

	for _, call := range rec.snapshot() {
		if call.method != "executeCdp" {
			continue
		}
		cmd, _ := call.params["commandParams"].(map[string]interface{})
		if cmd["type"] != "mouseWheel" {
			t.Errorf("type = %v, want mouseWheel", cmd["type"])
		}
		if cmd["deltaY"] != float64(100) {
			t.Errorf("deltaY = %v, want 100", cmd["deltaY"])
		}
	}
}

func TestDomCUAClickComputesBoxCenter(t *testing.T) {
	var clickX, clickY float64
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}
		}
		params, _ := req.Params.(map[string]interface{})
		switch params["method"] {
		case "DOM.getBoxModel":
			return map[string]interface{}{
				"model": map[string]interface{}{
					// content quad: 4 corners of a 100x50 box at (200, 300)
					"content": []float64{200, 300, 300, 300, 300, 350, 200, 350},
				},
			}
		case "Input.dispatchMouseEvent":
			cmd, _ := params["commandParams"].(map[string]interface{})
			clickX, _ = cmd["x"].(float64)
			clickY, _ = cmd["y"].(float64)
		}
		return map[string]bool{"ok": true}
	})
	defer cleanup()

	if err := c.DomCUAClick("3", "42"); err != nil {
		t.Fatalf("DomCUAClick: %v", err)
	}
	if clickX != 250 || clickY != 325 {
		t.Errorf("click center = (%v, %v), want (250, 325)", clickX, clickY)
	}
}

func TestDomCUAClickRejectsNonNumericNodeID(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} { return map[string]bool{"ok": true} })
	defer cleanup()
	if err := c.DomCUAClick("3", "not-a-number"); err == nil {
		t.Fatal("expected error for non-numeric node id, got nil")
	}
}

func TestDOMSnapshotPrimaryPath(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return map[string]interface{}{"nodes": []map[string]interface{}{{"role": "RootWebArea"}}}
	})
	defer cleanup()

	snap, err := c.DOMSnapshot("3")
	if err != nil {
		t.Fatalf("DOMSnapshot: %v", err)
	}
	if !strings.Contains(snap, "RootWebArea") {
		t.Errorf("snapshot missing AX content: %s", snap)
	}
}

func TestGetVisibleDOMReturnsValue(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		return map[string]interface{}{"result": map[string]interface{}{"value": "<body>...</body>"}}
	})
	defer cleanup()

	dom, err := c.GetVisibleDOM("3")
	if err != nil {
		t.Fatalf("GetVisibleDOM: %v", err)
	}
	if dom != "<body>...</body>" {
		t.Errorf("dom = %q", dom)
	}
}
