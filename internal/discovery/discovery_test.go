package discovery

import "testing"

func TestExtractUUID(t *testing.T) {
	tests := []struct {
		name string
		in   string
		want string
	}{
		{"hyphen separator with full uuid", "codex-browser-use-abc12345-6789-4def-9abc-123456789abc", "abc12345-6789-4def-9abc-123456789abc"},
		{"backslash separator", `codex-browser-use\abc12345-6789-4def-9abc-123456789abc`, "abc12345-6789-4def-9abc-123456789abc"},
		{"prefix only no separator", "codex-browser-use", ""},
		{"short suffix", "codex-browser-use-x", "x"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := extractUUID(tt.in); got != tt.want {
				t.Errorf("extractUUID(%q) = %q, want %q", tt.in, got, tt.want)
			}
		})
	}
}

func TestParsePipeList(t *testing.T) {
	output := "InputPipe_1\r\n" +
		"codex-browser-use-abc12345-6789-4def-9abc-123456789abc\r\n" +
		"   codex-browser-use-second-pipe   \r\n" +
		"unrelated-pipe\r\n" +
		"codex-browser-extra-foo\r\n"

	got := parsePipeList(output)
	if len(got) != 2 {
		t.Fatalf("expected 2 pipes, got %d: %+v", len(got), got)
	}
	if got[0].UUID != "abc12345-6789-4def-9abc-123456789abc" {
		t.Errorf("first pipe UUID = %q", got[0].UUID)
	}
	if got[1].Name != "codex-browser-use-second-pipe" {
		t.Errorf("second pipe name = %q (whitespace not trimmed)", got[1].Name)
	}
}

func TestPipePath(t *testing.T) {
	got := PipePath("codex-browser-use-foo")
	want := `\\.\pipe\codex-browser-use-foo`
	if got != want {
		t.Errorf("PipePath = %q, want %q", got, want)
	}
}
