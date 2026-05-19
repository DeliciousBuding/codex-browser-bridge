package mcp

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/DeliciousBuding/codex-browser-bridge/internal/client"
)

// MCPServer is an MCP stdio server that exposes browser automation tools.
type MCPServer struct {
	client  *client.Client
	tools   []Tool
	toolMap map[string]Tool
	in      io.Reader
	out     io.Writer
	version string
}

// Tool defines an MCP tool exposed by the server.
type Tool struct {
	Name        string                                        `json:"name"`
	Description string                                        `json:"description"`
	InputSchema json.RawMessage                               `json:"inputSchema"`
	Handler     func(args json.RawMessage) ([]Content, error) `json:"-"`
}

// Content is an MCP tool result content block.
type Content struct {
	Type     string `json:"type"`
	Text     string `json:"text,omitempty"`
	Data     string `json:"data,omitempty"`
	MimeType string `json:"mimeType,omitempty"`
}

func textContent(s string) []Content {
	return []Content{{Type: "text", Text: s}}
}

func imageContent(b64, mime string) Content {
	return Content{Type: "image", Data: b64, MimeType: mime}
}

// NewMCPServer creates an MCP server using os.Stdin and os.Stdout for transport.
func NewMCPServer(c *client.Client) *MCPServer {
	return NewMCPServerWithIO(c, os.Stdin, os.Stdout)
}

// NewMCPServerWithIO creates an MCP server with custom I/O streams.
func NewMCPServerWithIO(c *client.Client, in io.Reader, out io.Writer) *MCPServer {
	s := &MCPServer{
		client:  c,
		toolMap: make(map[string]Tool),
		in:      in,
		out:     out,
		version: "dev",
	}
	s.registerTools()
	return s
}

// SetVersion overrides the version reported in the MCP initialize handshake.
// main.go calls this with the ldflags-injected build version.
func (s *MCPServer) SetVersion(v string) {
	if v != "" {
		s.version = v
	}
}

func (s *MCPServer) registerTools() {
	s.tools = []Tool{
		// Tab management
		{Name: "codex_list_tabs", Description: "List all open browser tabs via Codex Chrome Extension",
			InputSchema: objectSchema(), Handler: s.handleListTabs},
		{Name: "codex_create_tab", Description: "Create a new browser tab",
			InputSchema: objectSchema(), Handler: s.handleCreateTab},
		{Name: "codex_close_tab", Description: "Close a browser tab",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to close"}},"required":["tab_id"]}`),
			Handler:     s.handleCloseTab},
		{Name: "codex_user_tabs", Description: "List user's open tabs across browser windows",
			InputSchema: objectSchema(), Handler: s.handleUserTabs},
		{Name: "codex_claim_tab", Description: "Claim a user tab for automation control",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}`),
			Handler:     s.handleClaimTab},

		// Navigation
		{Name: "codex_navigate", Description: "Navigate a tab to a URL",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"url":{"type":"string"}},"required":["tab_id","url"]}`),
			Handler:     s.handleNavigate},
		{Name: "codex_reload", Description: "Reload a tab",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}`),
			Handler:     s.handleReload},
		{Name: "codex_navigate_back", Description: "Navigate a tab back one entry in its history",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}`),
			Handler:     s.handleNavigateBack},
		{Name: "codex_navigate_forward", Description: "Navigate a tab forward one entry in its history",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}`),
			Handler:     s.handleNavigateForward},
		{Name: "codex_wait_for_load", Description: "Poll document.readyState until it equals \"complete\" or timeout (ms) elapses. Useful after navigation on slow pages.",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"timeout_ms":{"type":"number","description":"Timeout in milliseconds. Defaults to 10000."}},"required":["tab_id"]}`),
			Handler:     s.handleWaitForLoad},

		// Playwright API
		{Name: "codex_dom_snapshot", Description: "Get accessibility tree DOM snapshot of a tab",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}`),
			Handler:     s.handleDOMSnapshot},
		{Name: "codex_screenshot", Description: "Capture a screenshot. Returns image content viewable by the agent.",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"fullPage":{"type":"boolean"}},"required":["tab_id"]}`),
			Handler:     s.handleScreenshot},
		{Name: "codex_click", Description: "Click an element via Playwright selector",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"}},"required":["tab_id","selector"]}`),
			Handler:     s.handleClick},
		{Name: "codex_fill", Description: "Fill a form input via Playwright selector",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"},"value":{"type":"string"}},"required":["tab_id","selector","value"]}`),
			Handler:     s.handleFill},
		{Name: "codex_evaluate", Description: "Evaluate JavaScript in the page context",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"expression":{"type":"string"}},"required":["tab_id","expression"]}`),
			Handler:     s.handleEvaluate},

		// CUA (coordinate-based)
		{Name: "codex_cua_click", Description: "Click at screen coordinates (CUA)",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"}},"required":["tab_id","x","y"]}`),
			Handler:     s.handleCUAClick},
		{Name: "codex_cua_type", Description: "Type text at current focus (CUA)",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"text":{"type":"string"}},"required":["tab_id","text"]}`),
			Handler:     s.handleCUAType},
		{Name: "codex_cua_keypress", Description: "Press keyboard keys (CUA)",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"keys":{"type":"array","items":{"type":"string"}}},"required":["tab_id","keys"]}`),
			Handler:     s.handleCUAKeypress},
		{Name: "codex_cua_scroll", Description: "Scroll at coordinates (CUA)",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"scroll_x":{"type":"number"},"scroll_y":{"type":"number"}},"required":["tab_id","x","y","scroll_x","scroll_y"]}`),
			Handler:     s.handleCUAScroll},

		// DOM CUA
		{Name: "codex_dom_get_visible", Description: "Get visible DOM with node IDs for DOM-based interaction",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}`),
			Handler:     s.handleGetVisibleDOM},
		{Name: "codex_dom_click", Description: "Click a DOM node by its node ID",
			InputSchema: schema(`{"type":"object","properties":{"tab_id":{"type":"string"},"node_id":{"type":"string"}},"required":["tab_id","node_id"]}`),
			Handler:     s.handleDomClick},

		// Session
		{Name: "codex_name_session", Description: "Name the browser automation session",
			InputSchema: schema(`{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}`),
			Handler:     s.handleNameSession},
		{Name: "codex_finalize", Description: "Finalize and clean up tabs after session",
			InputSchema: objectSchema(), Handler: s.handleFinalize},

		// Diagnostic
		{Name: "codex_get_info", Description: "Get backend info from the Codex extension",
			InputSchema: objectSchema(), Handler: s.handleGetInfo},
	}

	for _, t := range s.tools {
		s.toolMap[t.Name] = t
	}
}

// Run reads JSON-RPC from stdin and writes responses to stdout (MCP stdio transport).
func (s *MCPServer) Run() error {
	reader := bufio.NewReaderSize(s.in, 10*1024*1024)
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return fmt.Errorf("read stdin: %w", err)
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		var req struct {
			JSONRPC string          `json:"jsonrpc"`
			ID      json.RawMessage `json:"id"`
			Method  string          `json:"method"`
			Params  json.RawMessage `json:"params"`
		}
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			s.writeError(nil, -32700, "Parse error")
			continue
		}

		s.handleMessage(req)
	}
}

func (s *MCPServer) handleMessage(req struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}) {
	switch req.Method {
	case "initialize":
		s.writeResult(req.ID, map[string]interface{}{
			"protocolVersion": "2024-11-05",
			"capabilities":    map[string]interface{}{"tools": map[string]interface{}{}},
			"serverInfo":      map[string]interface{}{"name": "codex-browser-bridge", "version": s.version},
		})
	case "tools/list":
		tools := make([]map[string]interface{}, len(s.tools))
		for i, t := range s.tools {
			tools[i] = map[string]interface{}{
				"name":        t.Name,
				"description": t.Description,
				"inputSchema": t.InputSchema,
			}
		}
		s.writeResult(req.ID, map[string]interface{}{"tools": tools})
	case "tools/call":
		s.handleToolCall(req.ID, req.Params)
	case "notifications/initialized":
		// Notification — no response per JSON-RPC 2.0 Section 4.1
	default:
		if len(req.ID) > 0 && string(req.ID) != "null" {
			s.writeError(req.ID, -32601, "Unknown method: "+req.Method)
		}
	}
}

func (s *MCPServer) handleToolCall(id json.RawMessage, params json.RawMessage) {
	var p struct {
		Name      string          `json:"name"`
		Arguments json.RawMessage `json:"arguments"`
	}
	if err := json.Unmarshal(params, &p); err != nil {
		s.writeError(id, -32602, "Invalid params")
		return
	}

	tool, ok := s.toolMap[p.Name]
	if !ok {
		s.writeError(id, -32601, "Tool not found: "+p.Name)
		return
	}

	content, err := tool.Handler(p.Arguments)
	if err != nil {
		s.writeResult(id, map[string]interface{}{
			"content": textContent("Error: " + err.Error()),
			"isError": true,
		})
		return
	}

	s.writeResult(id, map[string]interface{}{"content": content})
}

func (s *MCPServer) writeResult(id json.RawMessage, result interface{}) {
	resp := map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      id,
		"result":  result,
	}
	data, err := json.Marshal(resp)
	if err != nil {
		fmt.Fprintf(os.Stderr, "mcp marshal error: %v\n", err)
		return
	}
	fmt.Fprintln(s.out, string(data))
}

func (s *MCPServer) writeError(id json.RawMessage, code int, msg string) {
	resp := map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      id,
		"error":   map[string]interface{}{"code": code, "message": msg},
	}
	data, err := json.Marshal(resp)
	if err != nil {
		fmt.Fprintf(os.Stderr, "mcp marshal error: %v\n", err)
		return
	}
	fmt.Fprintln(s.out, string(data))
}

// --- Tool handler implementations ---

func (s *MCPServer) handleListTabs(_ json.RawMessage) ([]Content, error) {
	tabs, err := s.client.ListTabs()
	if err != nil {
		return nil, err
	}
	data, err := json.MarshalIndent(tabs, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("marshal tabs: %v", err)
	}
	return textContent(string(data)), nil
}

func (s *MCPServer) handleCreateTab(_ json.RawMessage) ([]Content, error) {
	id, err := s.client.CreateTab()
	if err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Created tab: %s", id)), nil
}

func (s *MCPServer) handleCloseTab(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.CloseTab(p.TabID); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Closed tab %s", p.TabID)), nil
}

func (s *MCPServer) handleUserTabs(_ json.RawMessage) ([]Content, error) {
	tabs, err := s.client.ListUserTabs()
	if err != nil {
		return nil, err
	}
	data, err := json.MarshalIndent(tabs, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("marshal tabs: %v", err)
	}
	return textContent(string(data)), nil
}

func (s *MCPServer) handleClaimTab(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	tab, err := s.client.ClaimUserTab(p.TabID)
	if err != nil {
		return nil, err
	}
	data, err := json.Marshal(tab)
	if err != nil {
		return nil, fmt.Errorf("marshal tab: %v", err)
	}
	return textContent(string(data)), nil
}

func (s *MCPServer) handleNavigate(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
		URL   string `json:"url"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.Navigate(p.TabID, p.URL); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Navigated tab %s to %s", p.TabID, p.URL)), nil
}

func (s *MCPServer) handleReload(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.Reload(p.TabID); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Reloaded tab %s", p.TabID)), nil
}

func (s *MCPServer) handleNavigateBack(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.NavigateBack(p.TabID); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Navigated tab %s back", p.TabID)), nil
}

func (s *MCPServer) handleNavigateForward(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.NavigateForward(p.TabID); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Navigated tab %s forward", p.TabID)), nil
}

func (s *MCPServer) handleWaitForLoad(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID     string `json:"tab_id"`
		TimeoutMs int    `json:"timeout_ms"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	state, err := s.client.WaitForLoad(p.TabID, p.TimeoutMs)
	if err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Tab %s reached readyState=%s", p.TabID, state)), nil
}

func (s *MCPServer) handleDOMSnapshot(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	snap, err := s.client.DOMSnapshot(p.TabID)
	if err != nil {
		return nil, err
	}
	return textContent(snap), nil
}

func (s *MCPServer) handleScreenshot(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID    string `json:"tab_id"`
		FullPage bool   `json:"fullPage"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	b64, err := s.client.Screenshot(p.TabID, p.FullPage)
	if err != nil {
		return nil, err
	}
	return []Content{
		imageContent(b64, "image/png"),
		{Type: "text", Text: fmt.Sprintf("Screenshot captured for tab %s (%d bytes base64)", p.TabID, len(b64))},
	}, nil
}

func (s *MCPServer) handleClick(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID    string `json:"tab_id"`
		Selector string `json:"selector"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.Click(p.TabID, p.Selector); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Clicked %s in tab %s", p.Selector, p.TabID)), nil
}

func (s *MCPServer) handleFill(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID    string `json:"tab_id"`
		Selector string `json:"selector"`
		Value    string `json:"value"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.Fill(p.TabID, p.Selector, p.Value); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Filled %s in tab %s", p.Selector, p.TabID)), nil
}

func (s *MCPServer) handleEvaluate(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID      string `json:"tab_id"`
		Expression string `json:"expression"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	result, err := s.client.Evaluate(p.TabID, p.Expression)
	if err != nil {
		return nil, err
	}
	return textContent(string(result)), nil
}

func (s *MCPServer) handleCUAClick(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
		X     int    `json:"x"`
		Y     int    `json:"y"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.CUAClick(p.TabID, p.X, p.Y); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("CUA click at (%d,%d) in tab %s", p.X, p.Y, p.TabID)), nil
}

func (s *MCPServer) handleCUAType(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
		Text  string `json:"text"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.CUAType(p.TabID, p.Text); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("CUA typed text in tab %s", p.TabID)), nil
}

func (s *MCPServer) handleCUAKeypress(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string   `json:"tab_id"`
		Keys  []string `json:"keys"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.CUAKeypress(p.TabID, p.Keys); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("CUA keypress %v in tab %s", p.Keys, p.TabID)), nil
}

func (s *MCPServer) handleCUAScroll(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID   string `json:"tab_id"`
		X       int    `json:"x"`
		Y       int    `json:"y"`
		ScrollX int    `json:"scroll_x"`
		ScrollY int    `json:"scroll_y"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.CUAScroll(p.TabID, p.X, p.Y, p.ScrollX, p.ScrollY); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("CUA scroll at (%d,%d) delta (%d,%d) in tab %s", p.X, p.Y, p.ScrollX, p.ScrollY, p.TabID)), nil
}

func (s *MCPServer) handleGetVisibleDOM(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID string `json:"tab_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	dom, err := s.client.GetVisibleDOM(p.TabID)
	if err != nil {
		return nil, err
	}
	return textContent(dom), nil
}

func (s *MCPServer) handleDomClick(args json.RawMessage) ([]Content, error) {
	var p struct {
		TabID  string `json:"tab_id"`
		NodeID string `json:"node_id"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.DomCUAClick(p.TabID, p.NodeID); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("DOM click node %s in tab %s", p.NodeID, p.TabID)), nil
}

func (s *MCPServer) handleNameSession(args json.RawMessage) ([]Content, error) {
	var p struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(args, &p); err != nil {
		return nil, fmt.Errorf("invalid arguments: %v", err)
	}
	if err := s.client.NameSession(p.Name); err != nil {
		return nil, err
	}
	return textContent(fmt.Sprintf("Session named: %s", p.Name)), nil
}

func (s *MCPServer) handleFinalize(_ json.RawMessage) ([]Content, error) {
	if err := s.client.FinalizeTabs(nil); err != nil {
		return nil, err
	}
	return textContent("Tabs finalized"), nil
}

func (s *MCPServer) handleGetInfo(_ json.RawMessage) ([]Content, error) {
	info, err := s.client.GetInfo()
	if err != nil {
		return nil, err
	}
	return textContent(string(info)), nil
}

func objectSchema() json.RawMessage {
	return json.RawMessage(`{"type":"object","properties":{}}`)
}

func schema(s string) json.RawMessage {
	return json.RawMessage(s)
}
