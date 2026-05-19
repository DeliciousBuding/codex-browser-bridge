package client

import "testing"

func TestTabNormalizeStringID(t *testing.T) {
	tab := Tab{RawID: "abc"}
	tab.normalize()
	if tab.ID != "abc" {
		t.Errorf("string ID: got %q", tab.ID)
	}
}

func TestTabNormalizeNumericID(t *testing.T) {
	tab := Tab{RawID: float64(42)}
	tab.normalize()
	if tab.ID != "42" {
		t.Errorf("numeric ID: got %q, want \"42\"", tab.ID)
	}
}

func TestTabNormalizeNilID(t *testing.T) {
	tab := Tab{RawID: nil}
	tab.normalize()
	if tab.ID != "<nil>" {
		t.Errorf("nil ID: got %q", tab.ID)
	}
}

func TestUserTabNormalize(t *testing.T) {
	tab := UserTab{RawID: float64(123)}
	tab.normalize()
	if tab.ID != "123" {
		t.Errorf("UserTab numeric ID: got %q", tab.ID)
	}
}

func TestNewUUIDFormat(t *testing.T) {
	u, err := newUUID()
	if err != nil {
		t.Fatalf("newUUID: %v", err)
	}
	if len(u) != 36 {
		t.Fatalf("UUID length = %d, want 36: %q", len(u), u)
	}
	for _, i := range []int{8, 13, 18, 23} {
		if u[i] != '-' {
			t.Errorf("position %d: got %q, want '-' (uuid: %s)", i, u[i], u)
		}
	}
	if u[14] != '4' {
		t.Errorf("UUID v4 marker missing: %s", u)
	}
	variant := u[19]
	if variant != '8' && variant != '9' && variant != 'a' && variant != 'b' {
		t.Errorf("UUID variant byte = %q, expected one of 8/9/a/b", variant)
	}
}

func TestNewUUIDUnique(t *testing.T) {
	seen := make(map[string]struct{}, 1000)
	for i := 0; i < 1000; i++ {
		u, err := newUUID()
		if err != nil {
			t.Fatalf("newUUID iteration %d: %v", i, err)
		}
		if _, dup := seen[u]; dup {
			t.Fatalf("duplicate UUID after %d iterations: %s", i, u)
		}
		seen[u] = struct{}{}
	}
}

func TestTruncate(t *testing.T) {
	if got := truncate("hello", 10); got != "hello" {
		t.Errorf("short string changed: %q", got)
	}
	if got := truncate("helloworld", 5); got != "hello..." {
		t.Errorf("long string: got %q", got)
	}
}
