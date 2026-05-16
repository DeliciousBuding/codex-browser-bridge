package client

import (
	"encoding/json"
	"fmt"
	"strconv"
)

// --- High-level browser API methods ---

// Tab represents an open browser tab.
type Tab struct {
	ID    string `json:"-"`
	RawID interface{} `json:"id"`
	URL   string `json:"url,omitempty"`
	Title string `json:"title,omitempty"`
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
	// First get the target info to find the targetId
	targets, err := c.executeCdp(id, "Target.getTargets", nil)
	if err != nil {
		// Fallback: try Page.close
		_, err2 := c.executeCdp(id, "Page.close", nil)
		return err2
	}
	// Try to find the targetId for this tab
	var targetInfo struct {
		TargetInfos []struct {
			TargetID string `json:"targetId"`
			TabID    int    `json:"tabId,omitempty"`
		} `json:"targetInfos"`
	}
	if json.Unmarshal(targets, &targetInfo) == nil {
		for _, t := range targetInfo.TargetInfos {
			if t.TabID == id {
				_, err = c.SendRequest("executeCdp", map[string]interface{}{
					"tabId": id,
					"method": "Target.closeTarget",
					"commandParams": map[string]interface{}{
						"targetId": t.TargetID,
					},
				})
				return err
			}
		}
	}
	_, err = c.executeCdp(id, "Page.close", nil)
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
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return fmt.Errorf("navigate requires numeric tab_id, got %q", tabID)
	}
	// Ensure tab is attached to this session first
	_, _ = c.SendRequest("attach", map[string]interface{}{
		"tabId": id,
	})
	_, err = c.SendRequest("executeCdp", map[string]interface{}{
		"tabId":         id,
		"method":        "Page.navigate",
		"commandParams": map[string]interface{}{"url": url},
	})
	return err
}

// NavigateBack navigates a tab back in history via CDP.
func (c *Client) NavigateBack(tabID string) error {
	id, err := strconv.Atoi(tabID)
	if err != nil {
		return err
	}
	raw, err := c.executeCdp(id, "Page.getNavigationHistory", nil)
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
	if history.CurrentIndex <= 0 {
		return fmt.Errorf("no previous page in history")
	}
	entryID := history.Entries[history.CurrentIndex-1].ID
	_, err = c.executeCdp(id, "Page.navigateToHistoryEntry", map[string]interface{}{
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
	raw, err := c.executeCdp(id, "Page.getNavigationHistory", nil)
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
	if history.CurrentIndex >= len(history.Entries)-1 {
		return fmt.Errorf("no next page in history")
	}
	entryID := history.Entries[history.CurrentIndex+1].ID
	_, err = c.executeCdp(id, "Page.navigateToHistoryEntry", map[string]interface{}{
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
	_, err = c.executeCdp(id, "Page.reload", nil)
	return err
}

// --- CDP helper ---

func (c *Client) executeCdp(tabID int, method string, params map[string]interface{}) (json.RawMessage, error) {
	if params == nil {
		params = map[string]interface{}{}
	}
	// Ensure tab is attached to this session first
	_, _ = c.SendRequest("attach", map[string]interface{}{
		"tabId": tabID,
	})
	return c.SendRequest("executeCdp", map[string]interface{}{
		"tabId":          tabID,
		"method":         method,
		"commandParams":  params,
	})
}

// --- Playwright API (via executeUnhandledCommand) ---

// DOMSnapshot returns an accessibility tree snapshot of the page.
func (c *Client) DOMSnapshot(tabID string) (string, error) {
	raw, err := c.executeUnhandledCommand("playwright_dom_snapshot", map[string]interface{}{
		"tab_id": tabID,
	})
	if err != nil {
		return "", err
	}
	var result struct {
		DOMSnapshot string `json:"dom_snapshot"`
	}
	if err := json.Unmarshal(raw, &result); err != nil {
		// Try as direct string
		var s string
		if err2 := json.Unmarshal(raw, &s); err2 == nil {
			return s, nil
		}
		return string(raw), nil
	}
	return result.DOMSnapshot, nil
}

// Click clicks an element identified by a Playwright selector.
func (c *Client) Click(tabID, selector string) error {
	_, err := c.executeUnhandledCommand("playwright_click", map[string]interface{}{
		"tab_id":   tabID,
		"selector": selector,
	})
	return err
}

// Fill fills a form input identified by a Playwright selector.
func (c *Client) Fill(tabID, selector, value string) error {
	_, err := c.executeUnhandledCommand("playwright_fill", map[string]interface{}{
		"tab_id":   tabID,
		"selector": selector,
		"value":    value,
	})
	return err
}

// Evaluate runs JavaScript in the page context and returns the result.
func (c *Client) Evaluate(tabID, expression string) (json.RawMessage, error) {
	return c.executeUnhandledCommand("playwright_evaluate", map[string]interface{}{
		"tab_id":     tabID,
		"expression": expression,
	})
}

// Screenshot captures a screenshot of the tab. Returns base64-encoded PNG.
func (c *Client) Screenshot(tabID string, fullPage bool) (string, error) {
	raw, err := c.executeUnhandledCommand("playwright_screenshot", map[string]interface{}{
		"tab_id":   tabID,
		"fullPage": fullPage,
	})
	if err != nil {
		return "", err
	}
	var result struct {
		Base64 string `json:"base64"`
	}
	if err := json.Unmarshal(raw, &result); err != nil {
		return string(raw), nil
	}
	return result.Base64, nil
}

// WaitForLoadState waits for a page load state (load, domcontentloaded, networkidle).
func (c *Client) WaitForLoadState(tabID, state string) error {
	_, err := c.executeUnhandledCommand("playwright_wait_for_load_state", map[string]interface{}{
		"tab_id": tabID,
		"state":  state,
	})
	return err
}

// --- CUA (Computer Use Agent) API ---

// CUAClick clicks at screen coordinates.
func (c *Client) CUAClick(tabID string, x, y int) error {
	_, err := c.executeUnhandledCommand("cua_click", map[string]interface{}{
		"tab_id": tabID,
		"x":      x,
		"y":      y,
	})
	return err
}

// CUAType types text at the current focus.
func (c *Client) CUAType(tabID, text string) error {
	_, err := c.executeUnhandledCommand("cua_type", map[string]interface{}{
		"tab_id": tabID,
		"text":   text,
	})
	return err
}

// CUAKeypress presses keyboard keys.
func (c *Client) CUAKeypress(tabID string, keys []string) error {
	_, err := c.executeUnhandledCommand("cua_keypress", map[string]interface{}{
		"tab_id": tabID,
		"keys":   keys,
	})
	return err
}

// CUAScroll scrolls at coordinates.
func (c *Client) CUAScroll(tabID string, x, y, scrollX, scrollY int) error {
	_, err := c.executeUnhandledCommand("cua_scroll", map[string]interface{}{
		"tab_id":   tabID,
		"x":        x,
		"y":        y,
		"scroll_x": scrollX,
		"scroll_y": scrollY,
	})
	return err
}

// --- DOM CUA API ---

// DomCUAClick clicks a DOM node by its backend node ID.
func (c *Client) DomCUAClick(tabID, nodeID string) error {
	_, err := c.executeUnhandledCommand("dom_cua_click", map[string]interface{}{
		"tab_id":  tabID,
		"node_id": nodeID,
	})
	return err
}

// GetVisibleDOM returns a visible DOM tree with node IDs for interaction.
func (c *Client) GetVisibleDOM(tabID string) (string, error) {
	raw, err := c.executeUnhandledCommand("dom_cua_get_visible_dom", map[string]interface{}{
		"tab_id": tabID,
	})
	if err != nil {
		return "", err
	}
	var s string
	if err := json.Unmarshal(raw, &s); err == nil {
		return s, nil
	}
	return string(raw), nil
}

// DomCUAType types into the currently focused element.
func (c *Client) DomCUAType(tabID, text string) error {
	_, err := c.executeUnhandledCommand("dom_cua_type", map[string]interface{}{
		"tab_id": tabID,
		"text":   text,
	})
	return err
}

// --- User Tab API ---

// UserTab represents a tab in the user's browser.
type UserTab struct {
	ID         string `json:"-"`
	RawID      interface{} `json:"id"`
	Title      string `json:"title,omitempty"`
	URL        string `json:"url,omitempty"`
	LastOpened string `json:"lastOpened,omitempty"`
	TabGroup   string `json:"tabGroup,omitempty"`
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
	return result, nil
}

// FinalizeTabs cleans up tabs after a session.
func (c *Client) FinalizeTabs(keep []map[string]interface{}) error {
	params := map[string]interface{}{}
	if keep != nil {
		params["keep"] = keep
	}
	_, err := c.SendRequest("finalizeTabs", params)
	return err
}

// GetInfo returns backend info from the extension.
func (c *Client) GetInfo() (json.RawMessage, error) {
	return c.SendRequest("getInfo", nil)
}

// --- executeUnhandledCommand helper ---

func (c *Client) executeUnhandledCommand(cmdType string, params map[string]interface{}) (json.RawMessage, error) {
	if params == nil {
		params = map[string]interface{}{}
	}
	params["type"] = cmdType
	return c.SendRequest("executeUnhandledCommand", params)
}
