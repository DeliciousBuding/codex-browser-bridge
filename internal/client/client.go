package client

import (
	"bufio"
	"context"
	cryptoRand "crypto/rand"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"sync"
	"sync/atomic"
	"time"

	"math/rand"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/discovery"
	"github.com/DeliciousBuding/codex-browser-bridge/internal/protocol"
)

// Client communicates with the Codex Chrome extension via a named pipe.
type Client struct {
	conn    net.Conn
	reader  *bufio.Reader
	writer  sync.Mutex
	nextID  atomic.Int64
	session protocol.SessionParams

	pendingMu sync.Mutex
	pending   map[int]chan *protocol.Response

	cdpMu    sync.Mutex
	cdpLocks map[int]*sync.Mutex

	ctx    context.Context
	cancel context.CancelFunc
	log    *log.Logger
}

// Connect dials a named pipe and returns a ready-to-use Client.
// If pipeName is empty, auto-discovers codex-browser-use-* pipes and tries each
// one until a successful connection is made.
func Connect(pipeName string, logger *log.Logger) (*Client, error) {
	if pipeName == "" {
		pipes, err := discovery.DiscoverCodexPipes()
		if err != nil {
			return nil, fmt.Errorf("discover pipes: %w", err)
		}
		if len(pipes) == 0 {
			return nil, fmt.Errorf("no codex-browser-use pipes found.\n" +
				"Checklist:\n" +
				"  1. Codex Desktop is running\n" +
				"  2. Chrome is running\n" +
				"  3. Codex Chrome Extension is installed and enabled\n" +
				"  4. The extension has connected to Codex Desktop (open a Codex chat once to trigger initialization)")
		}

		// The pipe prefix namespace is flat: any local process can create pipes with
		// the "codex-browser-use-" prefix. When multiple pipes exist, an attacker
		// could register a fake pipe before the legitimate Codex Desktop starts.
		if len(pipes) > 2 && logger != nil {
			logger.Printf("Warning: multiple codex-browser-use pipes found (%d). This may indicate stale or unauthorized pipes.", len(pipes))
		}

		// Try each pipe until one connects AND passes health check
		var lastErr error
		for _, p := range pipes {
			path := discovery.PipePath(p.Name)
			conn, err := dialNamedPipe(path)
			if err != nil {
				lastErr = err
				if logger != nil {
					logger.Printf("pipe %s failed: %v, trying next...", p.UUID, err)
				}
				continue
			}
			c := NewFromConn(conn, logger)
			// Health check with short timeout
			type healthResult struct {
				result json.RawMessage
				err    error
			}
			hc := make(chan healthResult, 1)
			go func() {
				r, e := c.SendRequest("getInfo", nil)
				hc <- healthResult{r, e}
			}()
			select {
			case hr := <-hc:
				if hr.err != nil {
					_ = c.Close()
					lastErr = hr.err
					if logger != nil {
						logger.Printf("pipe %s health check failed: %v, trying next...", p.UUID, hr.err)
					}
					continue
				}
				if logger != nil {
					logger.Printf("auto-discovered pipe: %s (verified, info=%s)", p.Name, truncate(string(hr.result), 120))
				}
			case <-time.After(5 * time.Second):
				_ = c.Close()
				lastErr = fmt.Errorf("health check timed out")
				continue
			}
			return c, nil
		}
		return nil, fmt.Errorf("all %d pipes failed; last error: %w. "+
			"Try: restart Codex Desktop, then re-open the Codex Chrome Extension",
			len(pipes), lastErr)
	}

	path := discovery.PipePath(pipeName)
	conn, err := dialNamedPipe(path)
	if err != nil {
		return nil, fmt.Errorf("dial pipe %s: %w. "+
			"This usually means the pipe is stale (Codex Desktop restarted) or the extension lost its host. "+
			"Try: restart Codex Desktop, then re-open the Codex Chrome Extension", path, err)
	}

	return NewFromConn(conn, logger), nil
}

// NewFromConn wraps an established net.Conn in a Client and starts the read loop.
// Useful for tests that want to drive Client over an in-memory pipe, or callers
// that establish the transport themselves.
func NewFromConn(conn net.Conn, logger *log.Logger) *Client {
	ctx, cancel := context.WithCancel(context.Background())

	sessionID, err := newUUID()
	if err != nil {
		if logger != nil {
			logger.Printf("newUUID failed, using fallback: %v", err)
		}
		sessionID = fallbackUUID()
	}
	turnID, err := newUUID()
	if err != nil {
		if logger != nil {
			logger.Printf("newUUID failed, using fallback: %v", err)
		}
		turnID = fallbackUUID()
	}

	c := &Client{
		conn:   conn,
		reader: bufio.NewReaderSize(conn, 256*1024),
		session: protocol.SessionParams{
			SessionID: sessionID,
			TurnID:    turnID,
		},
		pending:  make(map[int]chan *protocol.Response),
		cdpLocks: make(map[int]*sync.Mutex),
		ctx:      ctx,
		cancel:   cancel,
		log:      logger,
	}
	go c.readLoop()
	return c
}

// Close shuts down the connection.
func (c *Client) Close() error {
	c.cancel()
	return c.conn.Close()
}

// SendRequest sends a JSON-RPC request and waits for the response.
func (c *Client) SendRequest(method string, params map[string]interface{}) (json.RawMessage, error) {
	id := int(c.nextID.Add(1))

	// Merge session params into the request params
	fullParams := map[string]interface{}{
		"session_id": c.session.SessionID,
		"turn_id":    c.session.TurnID,
	}
	for k, v := range params {
		fullParams[k] = v
	}

	req := protocol.Request{
		JSONRPC: "2.0",
		ID:      id,
		Method:  method,
		Params:  fullParams,
	}

	ch := make(chan *protocol.Response, 1)
	c.pendingMu.Lock()
	c.pending[id] = ch
	c.pendingMu.Unlock()

	defer func() {
		c.pendingMu.Lock()
		delete(c.pending, id)
		c.pendingMu.Unlock()
	}()

	c.writer.Lock()
	err := protocol.EncodeFrame(c.conn, req)
	c.writer.Unlock()
	if err != nil {
		return nil, fmt.Errorf("send request %s: %w", method, err)
	}

	if c.log != nil {
		c.log.Printf("→ %s (id=%d)", method, id)
	}

	timer := time.NewTimer(60 * time.Second)
	defer timer.Stop()

	select {
	case resp := <-ch:
		if resp.Error != nil {
			return nil, fmt.Errorf("rpc error in %s: %s", method, resp.Error.Error())
		}
		return resp.Result, nil
	case <-c.ctx.Done():
		return nil, fmt.Errorf("connection closed while waiting for %s", method)
	case <-timer.C:
		return nil, fmt.Errorf("timeout waiting for %s response", method)
	}
}

// SendNotification sends a JSON-RPC notification (no response expected).
func (c *Client) SendNotification(method string, params interface{}) error {
	msg := struct {
		JSONRPC string      `json:"jsonrpc"`
		Method  string      `json:"method"`
		Params  interface{} `json:"params,omitempty"`
	}{
		JSONRPC: "2.0",
		Method:  method,
		Params:  params,
	}
	c.writer.Lock()
	defer c.writer.Unlock()
	return protocol.EncodeFrame(c.conn, msg)
}

func (c *Client) readLoop() {
	for {
		raw, err := protocol.DecodeFrame(c.reader)
		if err != nil {
			if c.ctx.Err() != nil {
				return // graceful shutdown
			}
			if c.log != nil {
				c.log.Printf("read error: %v", err)
			}
			c.cancel()
			return
		}

		// Try to parse as response (has "id" field)
		var resp protocol.Response
		if err := json.Unmarshal(raw, &resp); err == nil && resp.ID != nil {
			c.pendingMu.Lock()
			ch, ok := c.pending[*resp.ID]
			c.pendingMu.Unlock()
			if ok {
				select {
				case ch <- &resp:
				default:
					// duplicate or late response; drop
				}
			}
			continue
		}

		// Otherwise it's a notification or request from the server — log and ignore
		if c.log != nil {
			c.log.Printf("← notification: %s", truncate(string(raw), 200))
		}
	}
}

func (c *Client) lockTabCDP(tabID int) func() {
	c.cdpMu.Lock()
	mu, ok := c.cdpLocks[tabID]
	if !ok {
		mu = &sync.Mutex{}
		c.cdpLocks[tabID] = mu
	}
	c.cdpMu.Unlock()

	mu.Lock()
	return mu.Unlock
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "..."
}

// newUUID generates a UUID v4 string without external dependencies.
func newUUID() (string, error) {
	var b [16]byte
	if _, err := cryptoRand.Read(b[:]); err != nil {
		return "", fmt.Errorf("crypto/rand failed: %w", err)
	}
	b[6] = (b[6] & 0x0f) | 0x40 // version 4
	b[8] = (b[8] & 0x3f) | 0x80 // variant 10
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16]), nil
}

// fallbackUUID generates a UUID v4 using math/rand when crypto/rand is unavailable.
func fallbackUUID() string {
	var b [16]byte
	for i := range b {
		b[i] = byte(rand.Intn(256))
	}
	b[6] = (b[6] & 0x0f) | 0x40 // version 4
	b[8] = (b[8] & 0x3f) | 0x80 // variant 10
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
