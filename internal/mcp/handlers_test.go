package mcp

import (
	"bufio"
	"encoding/json"
	"net"
	"strings"
	"sync"
	"testing"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/client"
	"github.com/DeliciousBuding/codex-browser-bridge/internal/protocol"
)

type pipeRequest struct {
	Method string
	Params map[string]interface{}
}

type pipeServer struct {
	t       *testing.T
	conn    net.Conn
	reader  *bufio.Reader
	handler func(req protocol.Request) (interface{}, *protocol.ErrorObject)
	mu      sync.Mutex
	calls   []pipeRequest
	wg      sync.WaitGroup
}

func newPipeServer(t *testing.T, conn net.Conn, handler func(protocol.Request) (interface{}, *protocol.ErrorObject)) *pipeServer {
	s := &pipeServer{t: t, conn: conn, reader: bufio.NewReader(conn), handler: handler}
	s.wg.Add(1)
	go s.serve()
	return s
}

func (s *pipeServer) serve() {
	defer s.wg.Done()
	for {
		raw, err := protocol.DecodeFrame(s.reader)
		if err != nil {
			return
		}
		var req protocol.Request
		if err := json.Unmarshal(raw, &req); err != nil {
			return
		}
		s.mu.Lock()
		s.calls = append(s.calls, pipeRequest{Method: req.Method, Params: asMap(req.Params)})
		s.mu.Unlock()

		result, errObj := s.handler(req)
		var resultRaw json.RawMessage
		if result != nil {
			b, _ := json.Marshal(result)
			resultRaw = b
		}
		resp := protocol.Response{ID: req.ID, Result: resultRaw, Error: errObj}
		if err := protocol.EncodeFrame(s.conn, resp); err != nil {
			return
		}
	}
}

func (s *pipeServer) recordedMethods() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]string, len(s.calls))
	for i, c := range s.calls {
		out[i] = c.Method
	}
	return out
}

func (s *pipeServer) close() {
	_ = s.conn.Close()
	s.wg.Wait()
}

func asMap(v interface{}) map[string]interface{} {
	m, _ := v.(map[string]interface{})
	return m
}

func newServerWithPipe(t *testing.T, handler func(protocol.Request) (interface{}, *protocol.ErrorObject)) (*MCPServer, *pipeServer, func()) {
	t.Helper()
	clientConn, serverConn := net.Pipe()
	pipe := newPipeServer(t, serverConn, handler)
	c := client.NewFromConn(clientConn, nil)
	srv := NewMCPServerWithIO(c, nil, nil)
	cleanup := func() {
		pipe.close()
		c.Close()
	}
	return srv, pipe, cleanup
}

func callTool(t *testing.T, s *MCPServer, name string, args map[string]interface{}) (string, bool) {
	t.Helper()
	tool, ok := s.toolMap[name]
	if !ok {
		t.Fatalf("tool %q not registered", name)
	}
	raw, _ := json.Marshal(args)
	result, err := tool.Handler(raw)
	if err != nil {
		return err.Error(), false
	}
	return flattenContent(result), true
}

func callToolRaw(t *testing.T, s *MCPServer, name string, args map[string]interface{}) ([]Content, error) {
	t.Helper()
	tool, ok := s.toolMap[name]
	if !ok {
		t.Fatalf("tool %q not registered", name)
	}
	raw, _ := json.Marshal(args)
	return tool.Handler(raw)
}

func flattenContent(content []Content) string {
	parts := make([]string, 0, len(content))
	for _, c := range content {
		if c.Type == "text" {
			parts = append(parts, c.Text)
		} else if c.Type == "image" {
			parts = append(parts, "[image:"+c.MimeType+"]")
		}
	}
	return strings.Join(parts, "\n")
}

func TestHandleListTabs(t *testing.T) {
	srv, _, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return []map[string]interface{}{
			{"id": 1, "url": "https://a.test", "title": "A"},
			{"id": 2, "url": "https://b.test", "title": "B"},
		}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_list_tabs", nil)
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "https://a.test") || !strings.Contains(out, "https://b.test") {
		t.Errorf("output missing tab urls: %s", out)
	}
}

func TestHandleCreateTab(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]interface{}{"id": 99}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_create_tab", nil)
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "99") {
		t.Errorf("output should mention created id: %s", out)
	}
	if methods := pipe.recordedMethods(); len(methods) != 1 || methods[0] != "createTab" {
		t.Errorf("recorded methods = %v", methods)
	}
}

func TestHandleNavigate(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_navigate", map[string]interface{}{
		"tab_id": "5",
		"url":    "https://example.com",
	})
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "5") || !strings.Contains(out, "example.com") {
		t.Errorf("output = %s", out)
	}

	methods := pipe.recordedMethods()
	if len(methods) < 2 || methods[0] != "attach" || methods[1] != "executeCdp" {
		t.Errorf("expected attach + executeCdp, got %v", methods)
	}
}

func TestHandleNavigateBack(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}, nil
		}
		params, _ := req.Params.(map[string]interface{})
		if params["method"] == "Page.getNavigationHistory" {
			return map[string]interface{}{
				"currentIndex": 1,
				"entries": []map[string]interface{}{
					{"id": 100, "url": "https://a.test"},
					{"id": 101, "url": "https://b.test"},
				},
			}, nil
		}
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_navigate_back", map[string]interface{}{"tab_id": "5"})
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "back") {
		t.Errorf("output should mention back navigation: %s", out)
	}

	hasNavigate := false
	for _, m := range pipe.recordedMethods() {
		if m == "executeCdp" {
			hasNavigate = true
		}
	}
	if !hasNavigate {
		t.Errorf("expected executeCdp calls, got %v", pipe.recordedMethods())
	}
}

func TestHandleNavigateForward(t *testing.T) {
	srv, _, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		if req.Method != "executeCdp" {
			return map[string]bool{"ok": true}, nil
		}
		params, _ := req.Params.(map[string]interface{})
		if params["method"] == "Page.getNavigationHistory" {
			return map[string]interface{}{
				"currentIndex": 0,
				"entries": []map[string]interface{}{
					{"id": 100, "url": "https://a.test"},
					{"id": 101, "url": "https://b.test"},
				},
			}, nil
		}
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_navigate_forward", map[string]interface{}{"tab_id": "5"})
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "forward") {
		t.Errorf("output should mention forward navigation: %s", out)
	}
}

func TestHandleScreenshot(t *testing.T) {
	srv, _, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		if req.Method == "executeCdp" {
			return map[string]string{"data": "PNGDATA"}, nil
		}
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	content, err := callToolRaw(t, srv, "codex_screenshot", map[string]interface{}{"tab_id": "3"})
	if err != nil {
		t.Fatalf("handler errored: %v", err)
	}
	var img *Content
	for i := range content {
		if content[i].Type == "image" {
			img = &content[i]
			break
		}
	}
	if img == nil {
		t.Fatalf("no image content block returned: %+v", content)
	}
	if img.Data != "PNGDATA" {
		t.Errorf("image data = %q, want PNGDATA", img.Data)
	}
	if img.MimeType != "image/png" {
		t.Errorf("image mimeType = %q, want image/png", img.MimeType)
	}
}

func TestHandleEvaluate(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]interface{}{"result": map[string]interface{}{"value": "hello"}}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_evaluate", map[string]interface{}{
		"tab_id":     "1",
		"expression": "1+1",
	})
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "hello") {
		t.Errorf("output should include evaluated value: %s", out)
	}

	for _, c := range pipe.calls {
		if c.Method != "executeCdp" {
			continue
		}
		cmd, _ := c.Params["commandParams"].(map[string]interface{})
		if cmd["expression"] != "1+1" {
			t.Errorf("expression forwarded as %v", cmd["expression"])
		}
	}
}

func TestHandleClaimTabAutoAttaches(t *testing.T) {
	srv, pipe, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]interface{}{"id": 7, "url": "https://example.com"}, nil
	})
	defer cleanup()

	out, ok := callTool(t, srv, "codex_claim_tab", map[string]interface{}{"tab_id": "7"})
	if !ok {
		t.Fatalf("handler errored: %s", out)
	}
	if !strings.Contains(out, "7") {
		t.Errorf("output should reference claimed id: %s", out)
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

func TestHandlerPropagatesRPCError(t *testing.T) {
	srv, _, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return nil, &protocol.ErrorObject{Code: -1, Message: "extension offline"}
	})
	defer cleanup()

	tool := srv.toolMap["codex_list_tabs"]
	_, err := tool.Handler(nil)
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if !strings.Contains(err.Error(), "extension offline") {
		t.Errorf("error %q should contain extension message", err.Error())
	}
}

func TestHandleCloseTabRejectsNonNumeric(t *testing.T) {
	srv, _, cleanup := newServerWithPipe(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]bool{"ok": true}, nil
	})
	defer cleanup()

	tool := srv.toolMap["codex_close_tab"]
	args, _ := json.Marshal(map[string]string{"tab_id": "abc"})
	if _, err := tool.Handler(args); err == nil {
		t.Fatal("expected error for non-numeric tab id, got nil")
	}
}
