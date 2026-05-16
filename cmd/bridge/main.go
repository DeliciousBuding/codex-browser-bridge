package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"

	"github.com/user/codex-browser-bridge/internal/client"
	"github.com/user/codex-browser-bridge/internal/discovery"
	"github.com/user/codex-browser-bridge/internal/mcp"
)

func main() {
	mode := flag.String("mode", "mcp", "Mode: mcp (MCP server via stdio), cli (interactive), discover (list pipes)")
	pipe := flag.String("pipe", "", "Pipe name override (auto-discovers if empty)")
	flag.Parse()

	logger := log.New(os.Stderr, "[codex-bridge] ", log.LstdFlags)

	switch *mode {
	case "discover":
		runDiscover()
	case "mcp":
		runMCP(*pipe, logger)
	case "cli":
		runCLI(*pipe, logger)
	default:
		fmt.Fprintf(os.Stderr, "Unknown mode: %s\n", *mode)
		os.Exit(1)
	}
}

func runDiscover() {
	pipes, err := discovery.DiscoverCodexPipes()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
	if len(pipes) == 0 {
		fmt.Println("No codex-browser-use pipes found. Is Codex Desktop running?")
		os.Exit(1)
	}
	data, _ := json.MarshalIndent(pipes, "", "  ")
	fmt.Println(string(data))
}

func runMCP(pipeName string, logger *log.Logger) {
	c, err := client.Connect(pipeName, logger)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to connect: %v\n", err)
		os.Exit(1)
	}
	defer c.Close()

	logger.Println("Connected to Codex browser pipe")

	srv := mcp.NewMCPServer(c)
	if err := srv.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "MCP server error: %v\n", err)
		os.Exit(1)
	}
}

func runCLI(pipeName string, logger *log.Logger) {
	c, err := client.Connect(pipeName, logger)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to connect: %v\n", err)
		os.Exit(1)
	}
	defer c.Close()

	fmt.Println("Connected to Codex browser pipe")
	fmt.Println("Commands: tabs, create, close <id>, user-tabs, claim <id>, nav <id> <url>,")
	fmt.Println("          snapshot <id>, screenshot <id>, info, ping, try <method>, quit")

	scanner := newScanner()
	for {
		fmt.Print("> ")
		line := scanner.nextLine()
		if line == "" {
			continue
		}

		args := splitArgs(line)
		cmd := args[0]

		switch cmd {
		case "tabs":
			tabs, err := c.ListTabs()
			if err != nil {
				fmt.Printf("Error: %v\n", err)
				continue
			}
			for _, t := range tabs {
				fmt.Printf("  [%s] %s — %s\n", t.ID, t.Title, t.URL)
			}
		case "create":
			id, err := c.CreateTab()
			if err != nil {
				fmt.Printf("Error: %v\n", err)
				continue
			}
			fmt.Printf("Created tab: %s\n", id)
		case "close":
			if len(args) < 2 {
				fmt.Println("Usage: close <tab_id>")
				continue
			}
			if err := c.CloseTab(args[1]); err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Printf("Closed tab %s\n", args[1])
			}
		case "user-tabs":
			tabs, err := c.ListUserTabs()
			if err != nil {
				fmt.Printf("Error: %v\n", err)
				continue
			}
			for _, t := range tabs {
				fmt.Printf("  [%s] %s — %s (group: %s)\n", t.ID, t.Title, t.URL, t.TabGroup)
			}
		case "claim":
			if len(args) < 2 {
				fmt.Println("Usage: claim <tab_id>")
				continue
			}
			tab, err := c.ClaimUserTab(args[1])
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Printf("Claimed: [%s] %s — %s\n", tab.ID, tab.Title, tab.URL)
			}
		case "nav":
			if len(args) < 3 {
				fmt.Println("Usage: nav <tab_id> <url>")
				continue
			}
			if err := c.Navigate(args[1], args[2]); err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Printf("Navigated tab %s to %s\n", args[1], args[2])
			}
		case "snapshot":
			if len(args) < 2 {
				fmt.Println("Usage: snapshot <tab_id>")
				continue
			}
			snap, err := c.DOMSnapshot(args[1])
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Println(snap)
			}
		case "screenshot":
			if len(args) < 2 {
				fmt.Println("Usage: screenshot <tab_id>")
				continue
			}
			b64, err := c.Screenshot(args[1], false)
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Printf("Screenshot (%d bytes base64)\n", len(b64))
			}
		case "info":
			info, err := c.GetInfo()
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Println(string(info))
			}
		case "ping":
			raw, err := c.SendRequest("ping", nil)
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Println(string(raw))
			}
		case "try":
			if len(args) < 2 {
				fmt.Println("Usage: try <method> [json_params]")
				continue
			}
			// Everything after "try method " is the JSON params
			method := args[1]
			var params map[string]interface{}
			if len(line) > len("try ")+len(method)+1 {
				jsonStr := line[len("try ")+len(method)+1:]
				if err := json.Unmarshal([]byte(jsonStr), &params); err != nil {
					fmt.Printf("Invalid JSON params: %v\n", err)
					continue
				}
			}
			raw, err := c.SendRequest(method, params)
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Println(string(raw))
			}
		case "quit", "exit":
			return
		default:
			fmt.Printf("Unknown command: %s\n", cmd)
		}
	}
}

type scanner struct {
	buf []byte
}

func newScanner() *scanner {
	return &scanner{}
}

func (s *scanner) nextLine() string {
	var line []byte
	b := make([]byte, 1)
	for {
		n, err := os.Stdin.Read(b)
		if err != nil || n == 0 {
			return ""
		}
		if b[0] == '\n' {
			break
		}
		if b[0] != '\r' {
			line = append(line, b[0])
		}
	}
	return string(line)
}

func splitArgs(s string) []string {
	var args []string
	current := ""
	inQuote := false
	for _, ch := range s {
		switch {
		case ch == '"':
			inQuote = !inQuote
		case ch == ' ' && !inQuote:
			if current != "" {
				args = append(args, current)
				current = ""
			}
		default:
			current += string(ch)
		}
	}
	if current != "" {
		args = append(args, current)
	}
	return args
}
