package discovery

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

const codexPipePrefix = "codex-browser-use"

// PipeInfo holds the name and UUID of a discovered Codex named pipe.
type PipeInfo struct {
	Name string
	UUID string
}

// DiscoverCodexPipes lists Windows named pipes matching codex-browser-use*
// Uses [System.IO.Directory]::GetFiles instead of Get-ChildItem because newer
// Codex versions create pipes with backslash separators (e.g. "codex-browser-use\<uuid>")
// which Get-ChildItem treats as directories and skips.
func DiscoverCodexPipes() ([]PipeInfo, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "powershell", "-NoProfile", "-Command",
		"$d='\\\\.\\pipe\\'; [System.IO.Directory]::GetFileSystemEntries($d) | Where-Object { $_ -like '*codex-browser*' } | ForEach-Object { $_.Substring($d.Length) }")
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("enumerate pipes: %w", err)
	}
	return parsePipeList(string(out)), nil
}

func parsePipeList(output string) []PipeInfo {
	var pipes []PipeInfo
	for _, line := range strings.Split(output, "\n") {
		name := strings.TrimSpace(line)
		if !strings.HasPrefix(name, codexPipePrefix) {
			continue
		}
		uuid := extractUUID(name)
		if uuid == "" {
			continue
		}
		pipes = append(pipes, PipeInfo{Name: name, UUID: uuid})
	}
	return pipes
}

// extractUUID strips the "codex-browser-use" prefix (and separator) from a pipe name.
// Handles both old format "codex-browser-use-<uuid>" and new format "codex-browser-use\<uuid>".
func extractUUID(name string) string {
	rest := strings.TrimPrefix(name, codexPipePrefix)
	if len(rest) > 0 && (rest[0] == '-' || rest[0] == '\\') {
		rest = rest[1:]
	}
	return rest
}

// PipePath returns the full \\.\pipe\ path for a named pipe.
func PipePath(name string) string {
	return `\\.\pipe\` + name
}
