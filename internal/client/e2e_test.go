package client

import (
	"fmt"
	"strings"
	"testing"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/protocol"
)

// TestCdpWithAttachSequence verifies every CDP call goes through
// detach → attach → executeCdp to clear stale debugger state.
func TestCdpWithAttachSequence(t *testing.T) {
	tests := []struct {
		name string
		call func(c *Client) error
	}{
		{"Navigate", func(c *Client) error { return c.Navigate("1", "https://test.com") }},
		{"Screenshot", func(c *Client) error {
			b64, err := c.Screenshot("1", false)
			if err != nil {
				return err
			}
			if b64 == "" {
				return fmt.Errorf("expected non-empty screenshot data")
			}
			return nil
		}},
		{"Evaluate", func(c *Client) error {
			_, err := c.Evaluate("1", "1+1")
			return err
		}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c, rec, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
				if req.Method == "executeCdp" {
					return map[string]interface{}{
						"result": map[string]interface{}{"value": "ok"},
						"data":   "iVBORw0KGgo...",
					}
				}
				return map[string]bool{"ok": true}
			})
			defer cleanup()

			if err := tt.call(c); err != nil {
				t.Fatalf("%s: %v", tt.name, err)
			}

			calls := rec.snapshot()
			if len(calls) < 3 {
				t.Fatalf("expected detach+attach+executeCdp (3 calls), got %d: %v", len(calls), calls)
			}
			if calls[0].method != "detach" {
				t.Errorf("call[0] = %q, want detach", calls[0].method)
			}
			if calls[1].method != "attach" {
				t.Errorf("call[1] = %q, want attach", calls[1].method)
			}
			if calls[2].method != "executeCdp" {
				t.Errorf("call[2] = %q, want executeCdp", calls[2].method)
			}
		})
	}
}

// TestIsDebuggerError checks the error classification helper.
func TestIsDebuggerError(t *testing.T) {
	tests := []struct {
		msg  string
		want bool
	}{
		{"Debugger is not attached to the tab with id: 42", true},
		{"rpc error in executeCdp: Debugger is not attached", true},
		{"Debugger is not attached", true},
		{"connection refused", false},
		{"timeout", false},
		{"", false},
	}
	for _, tt := range tests {
		err := &testError{msg: tt.msg}
		got := isDebuggerError(err)
		if got != tt.want {
			t.Errorf("isDebuggerError(%q) = %v, want %v", tt.msg, got, tt.want)
		}
	}
}

type testError struct{ msg string }

func (e *testError) Error() string { return e.msg }

// TestWaitForLoadTimeout verifies WaitForLoad gives up after the deadline.
func TestWaitForLoadTimeout(t *testing.T) {
	c, _, cleanup := withRecordingServer(t, func(req protocol.Request) interface{} {
		if req.Method == "executeCdp" {
			return map[string]interface{}{
				"result": map[string]interface{}{"value": "loading"},
			}
		}
		return map[string]bool{"ok": true}
	})
	defer cleanup()

	state, err := c.WaitForLoad("1", 200)
	if err == nil {
		t.Fatalf("expected timeout error, got state=%q", state)
	}
	if err.Error() == "" || !strings.HasPrefix(err.Error(), "timed o") {
		t.Errorf("error = %v, should mention timeout", err)
	}
}
