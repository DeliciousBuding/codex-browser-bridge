package discovery

import (
	"fmt"
	"os/exec"
	"strings"
)

const codexPipePrefix = "codex-browser-use"

// PipeInfo holds the name and UUID of a discovered Codex named pipe.
type PipeInfo struct {
	Name string
	UUID string
}

// DiscoverCodexPipes lists Windows named pipes matching codex-browser-use-*
func DiscoverCodexPipes() ([]PipeInfo, error) {
	cmd := exec.Command("powershell", "-NoProfile", "-Command",
		"Get-ChildItem '\\\\.\\pipe\\' | Select-Object -ExpandProperty Name")
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
		pipes = append(pipes, PipeInfo{Name: name, UUID: extractUUID(name)})
	}
	return pipes
}

// extractUUID strips the "codex-browser-use-" prefix from a pipe name.
// Returns "" if the name has no separator after the prefix.
func extractUUID(name string) string {
	rest := strings.TrimPrefix(name, codexPipePrefix)
	rest = strings.TrimLeft(rest, "-\\")
	return rest
}

// PipePath returns the full \\.\pipe\ path for a named pipe.
func PipePath(name string) string {
	return `\\.\pipe\` + name
}
