package mcp

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func newTestServer(in string) (*MCPServer, *bytes.Buffer) {
	out := &bytes.Buffer{}
	s := NewMCPServerWithIO(nil, strings.NewReader(in), out)
	return s, out
}

func decodeResponses(t *testing.T, out *bytes.Buffer) []map[string]interface{} {
	t.Helper()
	var responses []map[string]interface{}
	for _, line := range strings.Split(strings.TrimSpace(out.String()), "\n") {
		if line == "" {
			continue
		}
		var resp map[string]interface{}
		if err := json.Unmarshal([]byte(line), &resp); err != nil {
			t.Fatalf("decode response %q: %v", line, err)
		}
		responses = append(responses, resp)
	}
	return responses
}

func TestRegisteredToolCount(t *testing.T) {
	s, _ := newTestServer("")
	const want = 22
	if len(s.tools) != want {
		t.Errorf("tool count = %d, want %d", len(s.tools), want)
	}
	if len(s.toolMap) != len(s.tools) {
		t.Errorf("toolMap size %d does not match tools slice %d", len(s.toolMap), len(s.tools))
	}
}

func TestRegisteredToolsHaveValidSchema(t *testing.T) {
	s, _ := newTestServer("")
	for _, tool := range s.tools {
		if tool.Name == "" {
			t.Errorf("tool with empty name: %+v", tool)
		}
		if tool.Description == "" {
			t.Errorf("tool %q has empty description", tool.Name)
		}
		var schema map[string]interface{}
		if err := json.Unmarshal(tool.InputSchema, &schema); err != nil {
			t.Errorf("tool %q has invalid JSON schema: %v", tool.Name, err)
		}
		if schema["type"] != "object" {
			t.Errorf("tool %q schema type = %v, want \"object\"", tool.Name, schema["type"])
		}
	}
}

func TestInitializeResponse(t *testing.T) {
	in := `{"jsonrpc":"2.0","id":1,"method":"initialize"}` + "\n"
	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}
	resps := decodeResponses(t, out)
	if len(resps) != 1 {
		t.Fatalf("expected 1 response, got %d", len(resps))
	}
	result, ok := resps[0]["result"].(map[string]interface{})
	if !ok {
		t.Fatalf("missing result: %+v", resps[0])
	}
	if result["protocolVersion"] != "2024-11-05" {
		t.Errorf("protocolVersion = %v", result["protocolVersion"])
	}
	info, _ := result["serverInfo"].(map[string]interface{})
	if info["name"] != "codex-browser-bridge" {
		t.Errorf("serverInfo.name = %v", info["name"])
	}
}

func TestToolsListResponse(t *testing.T) {
	in := `{"jsonrpc":"2.0","id":2,"method":"tools/list"}` + "\n"
	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}
	resps := decodeResponses(t, out)
	if len(resps) != 1 {
		t.Fatalf("expected 1 response, got %d", len(resps))
	}
	result, ok := resps[0]["result"].(map[string]interface{})
	if !ok {
		t.Fatalf("missing result: %+v", resps[0])
	}
	tools, ok := result["tools"].([]interface{})
	if !ok {
		t.Fatalf("tools field is %T, want []interface{}", result["tools"])
	}
	if len(tools) != len(s.tools) {
		t.Errorf("tools/list returned %d tools, server has %d", len(tools), len(s.tools))
	}
	first, _ := tools[0].(map[string]interface{})
	for _, key := range []string{"name", "description", "inputSchema"} {
		if _, present := first[key]; !present {
			t.Errorf("first tool missing %q field: %+v", key, first)
		}
	}
}

func TestUnknownMethodReturnsError(t *testing.T) {
	in := `{"jsonrpc":"2.0","id":3,"method":"nonexistent/method"}` + "\n"
	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}
	resps := decodeResponses(t, out)
	if len(resps) != 1 {
		t.Fatalf("expected 1 response, got %d", len(resps))
	}
	errObj, ok := resps[0]["error"].(map[string]interface{})
	if !ok {
		t.Fatalf("expected error object, got: %+v", resps[0])
	}
	if int(errObj["code"].(float64)) != -32601 {
		t.Errorf("error code = %v, want -32601", errObj["code"])
	}
}

func TestParseErrorOnInvalidJSON(t *testing.T) {
	in := "not valid json\n"
	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}
	resps := decodeResponses(t, out)
	if len(resps) != 1 {
		t.Fatalf("expected 1 response, got %d", len(resps))
	}
	errObj, ok := resps[0]["error"].(map[string]interface{})
	if !ok {
		t.Fatalf("expected error object, got: %+v", resps[0])
	}
	if int(errObj["code"].(float64)) != -32700 {
		t.Errorf("parse error code = %v, want -32700", errObj["code"])
	}
}

func TestUnknownToolReturnsError(t *testing.T) {
	in := `{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"codex_does_not_exist","arguments":{}}}` + "\n"
	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}
	resps := decodeResponses(t, out)
	if len(resps) != 1 {
		t.Fatalf("expected 1 response, got %d", len(resps))
	}
	errObj, ok := resps[0]["error"].(map[string]interface{})
	if !ok {
		t.Fatalf("expected error object, got: %+v", resps[0])
	}
	msg, _ := errObj["message"].(string)
	if !strings.Contains(msg, "codex_does_not_exist") {
		t.Errorf("error message %q should mention the tool name", msg)
	}
}

func TestEmptyLineIgnored(t *testing.T) {
	in := "\n\n" + `{"jsonrpc":"2.0","id":1,"method":"initialize"}` + "\n"
	s, out := newTestServer(in)
	if err := s.Run(); err != nil {
		t.Fatalf("Run: %v", err)
	}
	resps := decodeResponses(t, out)
	if len(resps) != 1 {
		t.Fatalf("expected 1 response (empty lines should be ignored), got %d", len(resps))
	}
}
