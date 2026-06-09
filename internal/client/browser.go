package client

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
	"time"
)

// --- High-level browser API methods ---

// Tab represents an open browser tab.
type Tab struct {
	ID    string      `json:"-"`
	RawID interface{} `json:"id"`
	URL   string      `json:"url,omitempty"`
	Title string      `json:"title,omitempty"`
}

func (t *Tab) normalize() {
	switch v := t.RawID.(type) {
	case string:
		t.ID = v
	case float64:
		t.ID = strconv.FormatFloat(v, 'f', -1, 64)
	default:
		t.ID = fmt.Sprintf("%v", t.RawID)
	}
}

// ListTabs returns all tabs managed by this session.
func (c *Client) ListTabs() ([]Tab, error) {
	raw, err := c.SendRequest("getTabs", nil)
	if err != nil {
		return nil, err
	}
	var tabs []Tab
	if err := json.Unmarshal(raw, &tabs); err != nil {
		return nil, fmt.Errorf("decode getTabs: %w", err)
	}
	for i := range tabs {
		tabs[i].normalize()
	}
	return tabs, nil
}

// CreateTab opens a new tab and returns its ID.
func (c *Client) CreateTab() (string, error) {
	raw, err := c.SendRequest("createTab", nil)
	if err != nil {
		return "", err
	}
	var result Tab
	if err := json.Unmarshal(raw, &result); err != nil {
		return "", fmt.Errorf("decode createTab: %w", err)
	}
	result.normalize()
	return result.ID, nil
}

// CloseTab closes a tab by its numeric ID via CDP.
func (c *Client) CloseTab(tabID string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return fmt.Errorf("close_tab requires numeric tab_id, got %q", tabID)
	}
	_, err = c.cdpWithAttach(id, "Page.close", nil)
	if err == nil {
		c.retireTabCDPLock(id)
	}
	return err
}

// NameSession assigns a human-readable name to the current browser session.
func (c *Client) NameSession(name string) error {
	_, err := c.SendRequest("nameSession", map[string]interface{}{
		"name": name,
	})
	return err
}

// Navigate navigates a tab to the given URL via CDP.
func (c *Client) Navigate(tabID, url string) error {
	if err := validateURL(url); err != nil {
		return err
	}
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return fmt.Errorf("navigate requires numeric tab_id, got %q", tabID)
	}
	_, err = c.cdpWithAttach(id, "Page.navigate", map[string]interface{}{
		"url": url,
	})
	return err
}

var blockedURLSchemes = []string{"file:", "javascript:", "data:", "vbscript:", "about:", "chrome:", "edge:"}

func validateURL(rawURL string) error {
	lower := strings.ToLower(strings.TrimSpace(rawURL))
	for _, scheme := range blockedURLSchemes {
		if strings.HasPrefix(lower, scheme) {
			return fmt.Errorf("blocked URL scheme %q", scheme)
		}
	}
	return nil
}

// NavigateBack navigates a tab back in history via CDP.
func (c *Client) NavigateBack(tabID string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	raw, err := c.cdpWithAttach(id, "Page.getNavigationHistory", nil)
	if err != nil {
		return err
	}
	var history struct {
		CurrentIndex int `json:"currentIndex"`
		Entries      []struct {
			ID  int    `json:"id"`
			URL string `json:"url"`
		} `json:"entries"`
	}
	if err := json.Unmarshal(raw, &history); err != nil {
		return err
	}
	if history.CurrentIndex <= 0 || history.CurrentIndex >= len(history.Entries) {
		return fmt.Errorf("no previous page in history")
	}
	entryID := history.Entries[history.CurrentIndex-1].ID
	_, err = c.cdpWithAttach(id, "Page.navigateToHistoryEntry", map[string]interface{}{
		"entryId": entryID,
	})
	return err
}

// NavigateForward navigates a tab forward in history via CDP.
func (c *Client) NavigateForward(tabID string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	raw, err := c.cdpWithAttach(id, "Page.getNavigationHistory", nil)
	if err != nil {
		return err
	}
	var history struct {
		CurrentIndex int `json:"currentIndex"`
		Entries      []struct {
			ID  int    `json:"id"`
			URL string `json:"url"`
		} `json:"entries"`
	}
	if err := json.Unmarshal(raw, &history); err != nil {
		return err
	}
	if history.CurrentIndex < 0 || history.CurrentIndex >= len(history.Entries)-1 {
		return fmt.Errorf("no next page in history")
	}
	entryID := history.Entries[history.CurrentIndex+1].ID
	_, err = c.cdpWithAttach(id, "Page.navigateToHistoryEntry", map[string]interface{}{
		"entryId": entryID,
	})
	return err
}

// Reload reloads a tab via CDP.
func (c *Client) Reload(tabID string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	_, err = c.cdpWithAttach(id, "Page.reload", nil)
	return err
}

// WaitForLoad polls document.readyState until it equals "complete" or timeoutMs elapses.
// Returns the final readyState observed; error if it never reached "complete".
func (c *Client) WaitForLoad(tabID string, timeoutMs int) (string, error) {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return "", fmt.Errorf("wait_for_load requires numeric tab_id, got %q", tabID)
	}
	if timeoutMs <= 0 {
		timeoutMs = 10000
	}
	deadline := time.Now().Add(time.Duration(timeoutMs) * time.Millisecond)
	last := ""
	for {
		if !time.Now().Before(deadline) {
			return last, fmt.Errorf("timed out after %dms waiting for readyState=complete (last=%q)", timeoutMs, last)
		}
		raw, err := c.cdpWithAttachUntil(id, "Runtime.evaluate", map[string]interface{}{
			"expression":    "document.readyState",
			"returnByValue": true,
		}, deadline)
		if err != nil {
			if isTransientLoadError(err) && time.Now().Before(deadline) {
				sleepUntil(deadline, 100*time.Millisecond)
				continue
			}
			return last, err
		}
		var result struct {
			Result struct {
				Value string `json:"value"`
			} `json:"result"`
		}
		if err := json.Unmarshal(raw, &result); err == nil {
			last = result.Result.Value
			if last == "complete" {
				return last, nil
			}
		}
		if time.Now().After(deadline) {
			return last, fmt.Errorf("timed out after %dms waiting for readyState=complete (last=%q)", timeoutMs, last)
		}
		sleepUntil(deadline, 100*time.Millisecond)
	}
}

// --- CDP helper ---

func (c *Client) executeCdpWithTimeout(tabID int, method string, params map[string]interface{}, timeout time.Duration) (json.RawMessage, error) {
	if params == nil {
		params = map[string]interface{}{}
	}
	return c.SendRequestWithTimeout("executeCdp", map[string]interface{}{
		"target": map[string]interface{}{
			"tabId": tabID,
		},
		"method":        method,
		"commandParams": params,
	}, timeout)
}

// attachTab attaches the debugger to a tab (required before CDP calls).
func (c *Client) attachTab(tabID int) error {
	return c.attachTabWithTimeout(tabID, defaultRequestTimeout)
}

func (c *Client) attachTabWithTimeout(tabID int, timeout time.Duration) error {
	_, err := c.SendRequestWithTimeout("attach", map[string]interface{}{
		"tabId": tabID,
	}, timeout)
	return err
}

// detachTab detaches the debugger from a tab.
func (c *Client) detachTab(tabID int) error {
	return c.detachTabWithTimeout(tabID, defaultRequestTimeout)
}

func (c *Client) detachTabWithTimeout(tabID int, timeout time.Duration) error {
	_, err := c.SendRequestWithTimeout("detach", map[string]interface{}{
		"tabId": tabID,
	}, timeout)
	return err
}

// cdpWithAttach ensures the debugger is attached and then executes a CDP command.
// It first tries to detach (to clear any stale attachment state), then attach
// fresh, then execute. If execute fails with "not attached", it retries once.
func (c *Client) cdpWithAttach(tabID int, method string, params map[string]interface{}) (json.RawMessage, error) {
	unlock := c.lockTabCDP(tabID)
	defer unlock()
	return c.cdpWithAttachLockedUntil(tabID, method, params, time.Time{})
}

func (c *Client) cdpWithAttachUntil(tabID int, method string, params map[string]interface{}, deadline time.Time) (json.RawMessage, error) {
	unlock := c.lockTabCDP(tabID)
	defer unlock()
	return c.cdpWithAttachLockedUntil(tabID, method, params, deadline)
}

func (c *Client) cdpWithAttachLockedUntil(tabID int, method string, params map[string]interface{}, deadline time.Time) (json.RawMessage, error) {
	// Detach first to clear any stale debugger state from Chrome
	_ = c.detachTabWithTimeout(tabID, requestTimeoutUntil(deadline))
	if err := c.attachTabWithTimeout(tabID, requestTimeoutUntil(deadline)); err != nil {
		return nil, fmt.Errorf("attach failed for tab %d: %w", tabID, err)
	}
	return c.executeCdpWithDebuggerRetryLockedUntil(tabID, method, params, deadline)
}

func (c *Client) executeCdpWithDebuggerRetryLockedUntil(tabID int, method string, params map[string]interface{}, deadline time.Time) (json.RawMessage, error) {
	raw, err := c.executeCdpWithTimeout(tabID, method, params, requestTimeoutUntil(deadline))
	if err != nil {
		// If attach didn't take, try one more detach+attach+retry cycle
		if isDebuggerError(err) {
			_ = c.detachTabWithTimeout(tabID, requestTimeoutUntil(deadline))
			if err2 := c.attachTabWithTimeout(tabID, requestTimeoutUntil(deadline)); err2 != nil {
				return nil, fmt.Errorf("retry attach failed for tab %d: %w", tabID, err2)
			}
			return c.executeCdpWithTimeout(tabID, method, params, requestTimeoutUntil(deadline))
		}
		return nil, err
	}
	return raw, nil
}

type cdpExecutor func(method string, params map[string]interface{}) (json.RawMessage, error)

func (c *Client) withAttachedCDP(tabID int, run func(cdpExecutor) error) error {
	unlock := c.lockTabCDP(tabID)
	defer unlock()

	_ = c.detachTab(tabID)
	if err := c.attachTab(tabID); err != nil {
		return fmt.Errorf("attach failed for tab %d: %w", tabID, err)
	}
	exec := func(method string, params map[string]interface{}) (json.RawMessage, error) {
		return c.executeCdpWithDebuggerRetryLockedUntil(tabID, method, params, time.Time{})
	}
	return run(exec)
}

func requestTimeoutUntil(deadline time.Time) time.Duration {
	if deadline.IsZero() {
		return defaultRequestTimeout
	}
	remaining := time.Until(deadline)
	if remaining <= 0 {
		return time.Nanosecond
	}
	return remaining
}

func sleepUntil(deadline time.Time, max time.Duration) {
	remaining := time.Until(deadline)
	if remaining <= 0 {
		return
	}
	if remaining < max {
		time.Sleep(remaining)
		return
	}
	time.Sleep(max)
}

func isDebuggerError(err error) bool {
	return err != nil && strings.Contains(err.Error(), "not attached")
}

func isTransientLoadError(err error) bool {
	if err == nil {
		return false
	}
	msg := strings.ToLower(err.Error())
	transient := []string{
		"execution context destroyed",
		"cannot find context with specified id",
		"inspected target navigated",
		"target closed",
		"frame was detached",
	}
	for _, s := range transient {
		if strings.Contains(msg, s) {
			return true
		}
	}
	return false
}

// jsonEscaped returns a JSON-escaped representation of s suitable for embedding
// in JavaScript string literals (e.g., inside Runtime.evaluate expressions).
// Unlike Go's %q, json.Marshal uses the same escaping rules as JavaScript.
func jsonEscaped(s string) string {
	b, err := json.Marshal(s)
	if err != nil {
		return `""`
	}
	return string(b)
}

// --- Playwright API (via CDP) ---

// DOMSnapshot returns an accessibility tree snapshot of the page.
func (c *Client) DOMSnapshot(tabID string) (string, error) {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return "", fmt.Errorf("snapshot requires numeric tab_id, got %q", tabID)
	}
	// Use CDP Accessibility.getFullAXTree for accessibility snapshot
	raw, err := c.cdpWithAttach(id, "Accessibility.getFullAXTree", nil)
	if err != nil {
		// Fallback: use Runtime.evaluate to get document.body text
		raw2, err2 := c.cdpWithAttach(id, "Runtime.evaluate", map[string]interface{}{
			"expression":    `document.body ? document.body.innerText : document.documentElement.innerText`,
			"returnByValue": true,
		})
		if err2 != nil {
			return "", fmt.Errorf("dom_snapshot failed: %v (fallback: %v)", err, err2)
		}
		var evalResult struct {
			Result struct {
				Value string `json:"value"`
			} `json:"result"`
		}
		if json.Unmarshal(raw2, &evalResult) == nil {
			return "/* fallback: plain text */\n" + evalResult.Result.Value, nil
		}
		return "/* fallback: plain text */\n" + string(raw2), nil
	}
	return string(raw), nil
}

// Screenshot captures a screenshot of the tab. Returns base64-encoded PNG.
// fullPage is reserved for a future implementation using Page.getLayoutMetrics
// + clip; currently always captures the viewport.
func (c *Client) Screenshot(tabID string, fullPage bool) (string, error) {
	_ = fullPage
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return "", fmt.Errorf("screenshot requires numeric tab_id, got %q", tabID)
	}
	raw, err := c.cdpWithAttach(id, "Page.captureScreenshot", map[string]interface{}{
		"format": "png",
	})
	if err != nil {
		return "", err
	}
	var result struct {
		Data string `json:"data"`
	}
	if err := json.Unmarshal(raw, &result); err != nil {
		return "", fmt.Errorf("parse screenshot response: %w", err)
	}
	return result.Data, nil
}

// --- CUA (Computer Use Agent) API via CDP ---

// CUAClick clicks at screen coordinates via CDP Input.dispatchMouseEvent.
func (c *Client) CUAClick(tabID string, x, y int) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	return c.withAttachedCDP(id, func(exec cdpExecutor) error {
		_, err := exec("Input.dispatchMouseEvent", map[string]interface{}{
			"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1,
		})
		if err != nil {
			return err
		}
		_, err = exec("Input.dispatchMouseEvent", map[string]interface{}{
			"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1,
		})
		return err
	})
}

// CUAType types text via CDP Input.insertText.
func (c *Client) CUAType(tabID, text string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	if text == "" {
		return nil
	}
	return c.withAttachedCDP(id, func(exec cdpExecutor) error {
		_, err := exec("Input.insertText", map[string]interface{}{"text": text})
		return err
	})
}

// CUAKeypress presses keyboard keys via CDP Input.dispatchKeyEvent.
func (c *Client) CUAKeypress(tabID string, keys []string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	return c.withAttachedCDP(id, func(exec cdpExecutor) error {
		for _, key := range keys {
			_, err := exec("Input.dispatchKeyEvent", map[string]interface{}{
				"type": "keyDown", "key": key,
			})
			if err != nil {
				return err
			}
			_, err = exec("Input.dispatchKeyEvent", map[string]interface{}{
				"type": "keyUp", "key": key,
			})
			if err != nil {
				return err
			}
		}
		return nil
	})
}

// CUAScroll scrolls at coordinates via CDP Input.dispatchMouseEvent.
func (c *Client) CUAScroll(tabID string, x, y, scrollX, scrollY int) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	_, err = c.cdpWithAttach(id, "Input.dispatchMouseEvent", map[string]interface{}{
		"type": "mouseWheel", "x": x, "y": y,
		"deltaX": float64(scrollX), "deltaY": float64(scrollY),
	})
	return err
}

// --- DOM CUA API via CDP ---

// DomCUAClick clicks a DOM node by its backend node ID via CDP.
func (c *Client) DomCUAClick(tabID, nodeID string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	nID, err := strconv.Atoi(nodeID)
	if err != nil {
		return err
	}
	// Resolve node to coordinates, then click
	_, err = c.cdpWithAttach(id, "DOM.resolveNode", map[string]interface{}{
		"backendNodeId": nID,
	})
	if err != nil {
		return err
	}
	// Get the node's box model and click center
	raw, err := c.cdpWithAttach(id, "DOM.getBoxModel", map[string]interface{}{
		"backendNodeId": nID,
	})
	if err != nil {
		return err
	}
	var box struct {
		Model struct {
			Content []float64 `json:"content"`
		} `json:"model"`
	}
	if err := json.Unmarshal(raw, &box); err != nil {
		return fmt.Errorf("parse box model: %w", err)
	}
	if len(box.Model.Content) < 8 {
		return fmt.Errorf("box model has insufficient content quads: got %d elements", len(box.Model.Content))
	}
	// Content quad: [x1,y1, x2,y2, x3,y3, x4,y4]. Center is average.
	cx := (box.Model.Content[0] + box.Model.Content[2] + box.Model.Content[4] + box.Model.Content[6]) / 4
	cy := (box.Model.Content[1] + box.Model.Content[3] + box.Model.Content[5] + box.Model.Content[7]) / 4
	return c.CUAClick(tabID, int(cx), int(cy))
}

// GetVisibleDOM returns a simplified DOM tree via CDP.
func (c *Client) GetVisibleDOM(tabID string) (string, error) {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return "", err
	}
	raw, err := c.cdpWithAttach(id, "Runtime.evaluate", map[string]interface{}{
		"expression": `(() => {
			function walk(node, depth) {
				if (depth > 5) return '';
				if (!node || node.nodeType !== 1) return '';
				const tag = node.tagName.toLowerCase();
				const id = node.id ? '#'+node.id : '';
				const cls = node.className ? '.'+String(node.className).replace(/\\s+/g,'.') : '';
				const text = node.childNodes.length === 1 && node.childNodes[0].nodeType === 3 ? node.childNodes[0].textContent.trim() : '';
				const rect = node.getBoundingClientRect();
				const vis = rect.width > 0 && rect.height > 0;
				if (!vis) return '';
				let line = '  '.repeat(depth) + '<' + tag + id + cls + '>';
				if (text) line += ' ' + text.slice(0,80);
				line += '\\n';
				for (const ch of node.children) line += walk(ch, depth+1);
				return line;
			}
			return walk(document.body, 0);
		})()`,
		"returnByValue": true,
	})
	if err != nil {
		return "", err
	}
	var result struct {
		Result struct {
			Value string `json:"value"`
		} `json:"result"`
	}
	if err := json.Unmarshal(raw, &result); err != nil {
		return string(raw), nil
	}
	return result.Result.Value, nil
}

// DomCUAType types into the currently focused element.
func (c *Client) DomCUAType(tabID, text string) error {
	return c.CUAType(tabID, text)
}

// --- High-level browser actions via CDP ---

// Click clicks an element by CSS selector via CDP.
func (c *Client) Click(tabID, selector string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	s := jsonEscaped(selector)
	js := fmt.Sprintf(`(function(){try{var el=document.querySelector(%s);if(!el)return JSON.stringify({error:'element not found: '+%s});el.click();return JSON.stringify({ok:true})}catch(e){return JSON.stringify({error:String(e&&e.message||e)})}})()`, s, s)
	raw, err := c.cdpWithAttach(id, "Runtime.evaluate", map[string]interface{}{
		"expression":    js,
		"returnByValue": true,
	})
	if err != nil {
		return err
	}
	var evalResult struct {
		Result struct {
			Value string `json:"value"`
		} `json:"result"`
	}
	if err := json.Unmarshal(raw, &evalResult); err != nil {
		return err
	}
	var clickResult struct {
		Ok    bool   `json:"ok"`
		Error string `json:"error"`
	}
	if err := json.Unmarshal([]byte(evalResult.Result.Value), &clickResult); err != nil {
		return err
	}
	if clickResult.Error != "" {
		return fmt.Errorf("click: %s", clickResult.Error)
	}
	return nil
}

// Fill fills a form input by CSS selector via CDP.
func (c *Client) Fill(tabID, selector, value string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	s := jsonEscaped(selector)
	v := jsonEscaped(value)
	js := fmt.Sprintf(`(function(){var el=document.querySelector(%s);if(!el)return JSON.stringify({error:'element not found: '+%s});el.focus();el.value=%s;el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));return JSON.stringify({ok:true})})()`, s, s, v)
	raw, err := c.cdpWithAttach(id, "Runtime.evaluate", map[string]interface{}{
		"expression": js,
	})
	if err != nil {
		return err
	}
	var evalResult struct {
		Result struct {
			Value string `json:"value"`
		} `json:"result"`
	}
	if err := json.Unmarshal(raw, &evalResult); err != nil {
		return err
	}
	var fillResult struct {
		Ok    bool   `json:"ok"`
		Error string `json:"error"`
	}
	if err := json.Unmarshal([]byte(evalResult.Result.Value), &fillResult); err != nil {
		return err
	}
	if fillResult.Error != "" {
		return fmt.Errorf("fill: %s", fillResult.Error)
	}
	return nil
}

// Evaluate runs JavaScript in the page context and returns the result.
func (c *Client) Evaluate(tabID, expression string) (json.RawMessage, error) {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return nil, err
	}
	return c.cdpWithAttach(id, "Runtime.evaluate", map[string]interface{}{
		"expression":    expression,
		"returnByValue": true,
	})
}

// --- User Tab API ---

// UserTab represents a tab in the user's browser.
type UserTab struct {
	ID         string      `json:"-"`
	RawID      interface{} `json:"id"`
	Title      string      `json:"title,omitempty"`
	URL        string      `json:"url,omitempty"`
	LastOpened string      `json:"lastOpened,omitempty"`
	TabGroup   string      `json:"tabGroup,omitempty"`
}

// normalize converts RawID to the string ID field.
func (t *UserTab) normalize() {
	switch v := t.RawID.(type) {
	case string:
		t.ID = v
	case float64:
		t.ID = strconv.FormatFloat(v, 'f', -1, 64)
	default:
		t.ID = fmt.Sprintf("%v", t.RawID)
	}
}

// ListUserTabs returns open tabs across the user's browser windows.
func (c *Client) ListUserTabs() ([]UserTab, error) {
	raw, err := c.SendRequest("getUserTabs", nil)
	if err != nil {
		return nil, err
	}
	// Try as wrapped {tabs: [...]} first
	var result struct {
		Tabs []UserTab `json:"tabs"`
	}
	if err := json.Unmarshal(raw, &result); err == nil && result.Tabs != nil {
		for i := range result.Tabs {
			result.Tabs[i].normalize()
		}
		return result.Tabs, nil
	}
	// Fallback: bare array
	var tabs []UserTab
	if err := json.Unmarshal(raw, &tabs); err != nil {
		return nil, fmt.Errorf("decode getUserTabs: %w", err)
	}
	for i := range tabs {
		tabs[i].normalize()
	}
	return tabs, nil
}

// ClaimUserTab takes over a user tab and returns it as a controllable tab.
func (c *Client) ClaimUserTab(tabID string) (Tab, error) {
	// Try numeric tabId first (Chrome extension API expects integer)
	tabIDInt, err := strconv.Atoi(tabID)
	if err != nil {
		return Tab{}, fmt.Errorf("claimUserTab requires numeric tab_id, got %q", tabID)
	}
	raw, err := c.SendRequest("claimUserTab", map[string]interface{}{
		"tabId": tabIDInt,
	})
	if err != nil {
		return Tab{}, err
	}
	var result Tab
	if err := json.Unmarshal(raw, &result); err != nil {
		return Tab{}, fmt.Errorf("decode claimUserTab: %w", err)
	}
	result.normalize()
	// Auto-attach debugger so CDP commands work immediately
	if err := c.attachTab(tabIDInt); err != nil && c.log != nil {
		c.log.Printf("claim tab %d: auto-attach failed: %v", tabIDInt, err)
	}
	return result, nil
}

// FinalizeTabs cleans up tabs after a session.
func (c *Client) FinalizeTabs(keep []map[string]interface{}) error {
	params := map[string]interface{}{}
	if keep != nil {
		params["keep"] = keep
	}
	_, err := c.SendRequest("finalizeTabs", params)
	if err == nil {
		c.retireAllCDPLocks()
	}
	return err
}

// GetInfo returns backend info from the extension.
func (c *Client) GetInfo() (json.RawMessage, error) {
	return c.SendRequest("getInfo", nil)
}
