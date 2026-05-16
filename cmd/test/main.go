package main

import (
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

	// 1. Get backend info
	info, err := c.GetInfo()
	if err != nil {
		fmt.Printf("GetInfo error: %v\n", err)
	} else {
		fmt.Printf("Backend: %s\n", string(info))
	}

	// 2. Create a new tab
	tabID, err := c.CreateTab()
	if err != nil {
		fmt.Printf("CreateTab error: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Created tab: %s\n", tabID)

	// 3. Navigate the tab
	err = c.Navigate(tabID, "https://example.com")
	if err != nil {
		fmt.Printf("Navigate error: %v\n", err)
	} else {
		fmt.Printf("Navigated tab %s to https://example.com\n", tabID)
	}

	// 4. Wait for page load
	time.Sleep(2 * time.Second)

	// 5. List tabs to verify
	tabs, err := c.ListTabs()
	if err != nil {
		fmt.Printf("ListTabs error: %v\n", err)
	} else {
		fmt.Printf("Tabs:\n")
		for _, t := range tabs {
			fmt.Printf("  [%s] %s — %s\n", t.ID, t.Title, t.URL)
		}
	}
}
