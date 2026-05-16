package client

import (
	"bufio"
	"encoding/json"
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
		resp := protocol.Response{ID: req.ID, Result: resultRaw, Error: errObj}
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
	c := newClient(clientConn, nil)
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

func contains(s, substr string) bool {
	for i := 0; i+len(substr) <= len(s); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
