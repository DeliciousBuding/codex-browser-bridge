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
