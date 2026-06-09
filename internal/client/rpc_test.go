package client

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"net"
	"sync"
	"testing"
	"time"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/protocol"
)

// fakeServer reads frames from conn and dispatches them to handler.
// handler receives the parsed request and returns a result or error to encode back.
type fakeServer struct {
	t       *testing.T
	conn    net.Conn
	reader  *bufio.Reader
	handler func(req protocol.Request) (interface{}, *protocol.ErrorObject)
	wg      sync.WaitGroup
}

func newFakeServer(t *testing.T, conn net.Conn, handler func(protocol.Request) (interface{}, *protocol.ErrorObject)) *fakeServer {
	s := &fakeServer{
		t:       t,
		conn:    conn,
		reader:  bufio.NewReader(conn),
		handler: handler,
	}
	s.wg.Add(1)
	go s.serve()
	return s
}

func (s *fakeServer) serve() {
	defer s.wg.Done()
	for {
		raw, err := protocol.DecodeFrame(s.reader)
		if err != nil {
			return
		}
		var req protocol.Request
		if err := json.Unmarshal(raw, &req); err != nil {
			s.t.Errorf("fake server: decode request: %v", err)
			return
		}
		result, errObj := s.handler(req)
		var resultRaw json.RawMessage
		if result != nil {
			b, _ := json.Marshal(result)
			resultRaw = b
		}
		id := req.ID
		resp := protocol.Response{ID: &id, Result: resultRaw, Error: errObj}
		if err := protocol.EncodeFrame(s.conn, resp); err != nil {
			return
		}
	}
}

func (s *fakeServer) close() {
	_ = s.conn.Close()
	s.wg.Wait()
}

func newPipedClient(t *testing.T, handler func(protocol.Request) (interface{}, *protocol.ErrorObject)) (*Client, *fakeServer) {
	t.Helper()
	clientConn, serverConn := net.Pipe()
	srv := newFakeServer(t, serverConn, handler)
	c := NewFromConn(clientConn, nil)
	return c, srv
}

func TestSendRequestEchoesResult(t *testing.T) {
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		if req.Method != "ping" {
			t.Errorf("unexpected method %q", req.Method)
		}
		return map[string]string{"pong": "ok"}, nil
	})
	defer srv.close()
	defer c.Close()

	raw, err := c.SendRequest("ping", nil)
	if err != nil {
		t.Fatalf("SendRequest: %v", err)
	}
	var got map[string]string
	if err := json.Unmarshal(raw, &got); err != nil {
		t.Fatalf("decode result: %v", err)
	}
	if got["pong"] != "ok" {
		t.Errorf("result = %v", got)
	}
}

func TestSendRequestInjectsSessionParams(t *testing.T) {
	var captured map[string]interface{}
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		captured, _ = req.Params.(map[string]interface{})
		return map[string]bool{"ok": true}, nil
	})
	defer srv.close()
	defer c.Close()

	if _, err := c.SendRequest("getInfo", map[string]interface{}{"extra": "value"}); err != nil {
		t.Fatalf("SendRequest: %v", err)
	}
	if captured["session_id"] == "" || captured["turn_id"] == "" {
		t.Errorf("session_id/turn_id missing: %+v", captured)
	}
	if captured["extra"] != "value" {
		t.Errorf("user param lost: %+v", captured)
	}
}

func TestSendRequestPropagatesRPCError(t *testing.T) {
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return nil, &protocol.ErrorObject{Code: -32601, Message: "no such method"}
	})
	defer srv.close()
	defer c.Close()

	_, err := c.SendRequest("bogus", nil)
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if msg := err.Error(); !contains(msg, "no such method") {
		t.Errorf("error %q should contain server message", msg)
	}
}

func TestSendRequestAssignsUniqueIDs(t *testing.T) {
	var seen []int
	var mu sync.Mutex
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		mu.Lock()
		seen = append(seen, req.ID)
		mu.Unlock()
		return map[string]bool{"ok": true}, nil
	})
	defer srv.close()
	defer c.Close()

	for i := 0; i < 5; i++ {
		if _, err := c.SendRequest("ping", nil); err != nil {
			t.Fatalf("SendRequest %d: %v", i, err)
		}
	}
	if len(seen) != 5 {
		t.Fatalf("expected 5 requests, got %d", len(seen))
	}
	uniq := make(map[int]struct{})
	for _, id := range seen {
		uniq[id] = struct{}{}
	}
	if len(uniq) != 5 {
		t.Errorf("ids not unique: %v", seen)
	}
}

func TestSendRequestTimesOutOnClosedConn(t *testing.T) {
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]bool{"ok": true}, nil
	})
	srv.close()

	done := make(chan error, 1)
	go func() {
		_, err := c.SendRequest("ping", nil)
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil {
			t.Fatal("expected error after server close, got nil")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("SendRequest did not return after server close")
	}
	c.Close()
}

func TestSendRequestConcurrent(t *testing.T) {
	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		return map[string]int{"echo": req.ID}, nil
	})
	defer srv.close()
	defer c.Close()

	const n = 32
	var wg sync.WaitGroup
	errs := make(chan error, n)
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			raw, err := c.SendRequest("ping", nil)
			if err != nil {
				errs <- err
				return
			}
			var got map[string]int
			if err := json.Unmarshal(raw, &got); err != nil {
				errs <- err
				return
			}
			if got["echo"] == 0 {
				errs <- fmt.Errorf("missing echoed id: %v", got)
			}
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		t.Errorf("concurrent request: %v", err)
	}
}

func contains(s, substr string) bool {
	for i := 0; i+len(substr) <= len(s); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

// TestSendNotificationFrame verifies that SendNotification encodes a
// valid JSON-RPC notification frame: no "id" field, correct length prefix,
// method and params properly encoded.
func TestSendNotificationFrame(t *testing.T) {
	clientConn, serverConn := net.Pipe()
	defer clientConn.Close()
	defer serverConn.Close()

	c := NewFromConn(clientConn, nil)
	defer c.Close()

	serverReader := bufio.NewReader(serverConn)

	type notifMsg struct {
		JSONRPC string      `json:"jsonrpc"`
		Method  string      `json:"method"`
		Params  interface{} `json:"params,omitempty"`
	}

	var got notifMsg
	errCh := make(chan error, 1)
	go func() {
		raw, err := protocol.DecodeFrame(serverReader)
		if err != nil {
			errCh <- fmt.Errorf("decode frame: %w", err)
			return
		}
		// Verify no "id" field in raw JSON (JSON-RPC notification)
		if bytes.Contains(raw, []byte(`"id"`)) {
			errCh <- fmt.Errorf("notification frame contains 'id' field: %s", raw)
			return
		}
		if err := json.Unmarshal(raw, &got); err != nil {
			errCh <- fmt.Errorf("unmarshal: %w", err)
			return
		}
		errCh <- nil
	}()

	err := c.SendNotification("test.event", map[string]string{"hello": "world"})
	if err != nil {
		t.Fatalf("SendNotification: %v", err)
	}

	if err := <-errCh; err != nil {
		t.Fatal(err)
	}
	if got.JSONRPC != "2.0" {
		t.Errorf("jsonrpc = %q, want \"2.0\"", got.JSONRPC)
	}
	if got.Method != "test.event" {
		t.Errorf("method = %q, want \"test.event\"", got.Method)
	}
	paramsMap, ok := got.Params.(map[string]interface{})
	if !ok {
		t.Fatalf("params is %T, want map", got.Params)
	}
	if v, ok := paramsMap["hello"].(string); !ok || v != "world" {
		t.Errorf("params[hello] = %v, want \"world\"", paramsMap["hello"])
	}
}

// TestSendRequestReturnsErrorOnConnectionClose verifies that a SendRequest
// in-flight returns an error (not hangs) when the server-side connection is
// closed while waiting for a response. Exercises the ctx.Done() path.
func TestSendRequestReturnsErrorOnConnectionClose(t *testing.T) {
	start := make(chan struct{})
	ready := make(chan struct{})

	c, srv := newPipedClient(t, func(req protocol.Request) (interface{}, *protocol.ErrorObject) {
		close(ready)
		<-start
		return map[string]bool{"ok": true}, nil
	})

	errCh := make(chan error, 1)
	go func() {
		_, err := c.SendRequest("ping", nil)
		errCh <- err
	}()

	<-ready          // wait until handler is invoked (request is in-flight)
	srv.conn.Close() // close server-side connection while request is waiting

	select {
	case err := <-errCh:
		if err == nil {
			t.Fatal("expected error after server close, got nil")
		}
		if err.Error() == "" {
			t.Error("error message should not be empty")
		}
		t.Logf("connection close error: %v", err)
	case <-time.After(5 * time.Second):
		t.Fatal("SendRequest did not return after server close (hung)")
	}

	close(start) // unblock handler goroutine
	srv.close()  // wait for handler goroutine to exit
	c.Close()
}
