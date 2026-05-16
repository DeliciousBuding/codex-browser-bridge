package discovery

import (
	"fmt"
	"os/exec"
	"strings"
)

type PipeInfo struct {
	Name string // Full pipe name, e.g. "codex-browser-use-abc123"
	UUID string // UUID portion if present
}

// DiscoverCodexPipes lists Windows named pipes matching codex-browser-use-*
func DiscoverCodexPipes() ([]PipeInfo, error) {
	cmd := exec.Command("powershell", "-NoProfile", "-Command",
		"Get-ChildItem '\\\\.\\pipe\\' | Select-Object -ExpandProperty Name")
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("enumerate pipes: %w", err)
	}
	var pipes []PipeInfo
	for _, line := range strings.Split(string(out), "\n") {
		name := strings.TrimSpace(line)
		if name == "" {
			continue
		}
		if strings.HasPrefix(name, "codex-browser-use") {
			uuid := ""
			// Pipe names: "codex-browser-use-<uuid>" or "codex-browser-use\<uuid>"
			if idx := strings.LastIndexAny(name, "-\\"); idx >= 0 && idx < len(name)-1 {
				uuid = name[idx+1:]
			}
			pipes = append(pipes, PipeInfo{Name: name, UUID: uuid})
		}
	}
	return pipes, nil
}

// PipePath returns the full named pipe path for connecting via net.Dial
func PipePath(name string) string {
	return `\\.\pipe\` + name
}
