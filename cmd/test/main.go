package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"time"

	"github.com/user/codex-browser-bridge/internal/client"
)

func main() {
	logger := log.New(os.Stderr, "[test] ", log.LstdFlags)

	c, err := client.Connect("", logger)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Connect failed: %v\n", err)
		os.Exit(1)
	}
	defer c.Close()

	// 1. Create tab
	tabID, err := c.CreateTab()
	if err != nil {
		fmt.Printf("FAIL createTab: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("OK createTab: %s\n", tabID)

	// 2. Navigate
	err = c.Navigate(tabID, "https://example.com")
	if err != nil {
		fmt.Printf("FAIL navigate: %v\n", err)
	} else {
		fmt.Printf("OK navigate\n")
	}

	// 3. Wait for load
	time.Sleep(2 * time.Second)

	// 4. List tabs
	tabs, err := c.ListTabs()
	if err != nil {
		fmt.Printf("FAIL listTabs: %v\n", err)
	} else {
		for _, t := range tabs {
			fmt.Printf("OK tab: [%s] %s — %s\n", t.ID, t.Title, t.URL)
		}
	}

	// 5. Screenshot
	b64, err := c.Screenshot(tabID, false)
	if err != nil {
		fmt.Printf("FAIL screenshot: %v\n", err)
	} else {
		fmt.Printf("OK screenshot: %d bytes base64\n", len(b64))
	}

	// 6. DOM snapshot
	snap, err := c.DOMSnapshot(tabID)
	if err != nil {
		fmt.Printf("FAIL domSnapshot: %v\n", err)
	} else {
		if len(snap) > 200 {
			snap = snap[:200] + "..."
		}
		fmt.Printf("OK domSnapshot: %s\n", snap)
	}

	// 7. Evaluate JS
	raw, err := c.Evaluate(tabID, "document.title")
	if err != nil {
		fmt.Printf("FAIL evaluate: %v\n", err)
	} else {
		var result struct {
			Result struct {
				Value string `json:"value"`
			} `json:"result"`
		}
		if json.Unmarshal(raw, &result) == nil {
			fmt.Printf("OK evaluate: document.title = %q\n", result.Result.Value)
		} else {
			fmt.Printf("OK evaluate: %s\n", string(raw)[:100])
		}
	}

	// 8. Close tab
	err = c.CloseTab(tabID)
	if err != nil {
		fmt.Printf("FAIL closeTab: %v\n", err)
	} else {
		fmt.Printf("OK closeTab\n")
	}
}
