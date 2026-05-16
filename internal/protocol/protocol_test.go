package protocol

import (
	"bytes"
	"encoding/binary"
	"strings"
	"testing"
)

func TestEncodeFrameLayout(t *testing.T) {
	var buf bytes.Buffer
	if err := EncodeFrame(&buf, map[string]string{"hello": "world"}); err != nil {
		t.Fatalf("EncodeFrame: %v", err)
	}
	raw := buf.Bytes()
	if len(raw) < 4 {
		t.Fatalf("frame too short: %d bytes", len(raw))
	}
	declared := binary.LittleEndian.Uint32(raw[:4])
	if int(declared) != len(raw)-4 {
		t.Errorf("length prefix %d does not match payload %d", declared, len(raw)-4)
	}
	if !bytes.Contains(raw[4:], []byte(`"hello":"world"`)) {
		t.Errorf("payload missing expected json: %s", raw[4:])
	}
}

func TestDecodeFrameRoundTrip(t *testing.T) {
	var buf bytes.Buffer
	original := Request{JSONRPC: "2.0", ID: 7, Method: "ping"}
	if err := EncodeFrame(&buf, original); err != nil {
		t.Fatalf("EncodeFrame: %v", err)
	}
	payload, err := DecodeFrame(&buf)
	if err != nil {
		t.Fatalf("DecodeFrame: %v", err)
	}
	got := string(payload)
	if !strings.Contains(got, `"id":7`) || !strings.Contains(got, `"method":"ping"`) {
		t.Errorf("round trip lost fields: %s", got)
	}
}

func TestDecodeFrameRejectsOversize(t *testing.T) {
	var hdr [4]byte
	binary.LittleEndian.PutUint32(hdr[:], 11*1024*1024)
	r := bytes.NewReader(hdr[:])
	if _, err := DecodeFrame(r); err == nil {
		t.Fatal("expected error for oversized frame, got nil")
	}
}

func TestDecodeFrameTruncatedHeader(t *testing.T) {
	r := bytes.NewReader([]byte{0x01, 0x02})
	if _, err := DecodeFrame(r); err == nil {
		t.Fatal("expected error for truncated header, got nil")
	}
}

func TestDecodeFrameTruncatedPayload(t *testing.T) {
	var buf bytes.Buffer
	binary.Write(&buf, binary.LittleEndian, uint32(10))
	buf.Write([]byte("short"))
	if _, err := DecodeFrame(&buf); err == nil {
		t.Fatal("expected error for truncated payload, got nil")
	}
}

func TestErrorObjectFormat(t *testing.T) {
	e := &ErrorObject{Code: -32601, Message: "method not found"}
	got := e.Error()
	if !strings.Contains(got, "-32601") || !strings.Contains(got, "method not found") {
		t.Errorf("Error() = %q", got)
	}
}
