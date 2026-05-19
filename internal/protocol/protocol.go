package protocol

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
)

// JSON-RPC 2.0 message types (Codex uses JSON-RPC without the "jsonrpc":"2.0" field on responses)

// Request is a JSON-RPC 2.0 request message.
type Request struct {
	JSONRPC string      `json:"jsonrpc"`
	ID      int         `json:"id"`
	Method  string      `json:"method"`
	Params  interface{} `json:"params,omitempty"`
}

// Response is a JSON-RPC 2.0 response message.
type Response struct {
	ID     *int            `json:"id"`
	Result json.RawMessage `json:"result,omitempty"`
	Error  *ErrorObject    `json:"error,omitempty"`
}

// ErrorObject carries a JSON-RPC error.
type ErrorObject struct {
	Code    int         `json:"code"`
	Message string      `json:"message"`
	Data    interface{} `json:"data,omitempty"`
}

func (e *ErrorObject) Error() string {
	return fmt.Sprintf("json-rpc error %d: %s", e.Code, e.Message)
}

// SessionParams are injected into every request's params
type SessionParams struct {
	SessionID string `json:"session_id"`
	TurnID    string `json:"turn_id"`
}

// EncodeFrame writes a length-prefixed JSON frame: [4-byte LE uint32][json bytes]
func EncodeFrame(w io.Writer, msg interface{}) error {
	payload, err := json.Marshal(msg)
	if err != nil {
		return fmt.Errorf("marshal frame: %w", err)
	}
	var lenBuf [4]byte
	binary.LittleEndian.PutUint32(lenBuf[:], uint32(len(payload)))
	if _, err := w.Write(lenBuf[:]); err != nil {
		return fmt.Errorf("write frame length: %w", err)
	}
	if _, err := w.Write(payload); err != nil {
		return fmt.Errorf("write frame payload: %w", err)
	}
	return nil
}

// DecodeFrame reads a single length-prefixed JSON frame from the reader.
func DecodeFrame(r io.Reader) (json.RawMessage, error) {
	var lenBuf [4]byte
	if _, err := io.ReadFull(r, lenBuf[:]); err != nil {
		return nil, fmt.Errorf("read frame length: %w", err)
	}
	length := binary.LittleEndian.Uint32(lenBuf[:])
	if length > 10*1024*1024 { // 10MB safety limit
		return nil, fmt.Errorf("frame too large: %d bytes", length)
	}
	payload := make([]byte, length)
	if _, err := io.ReadFull(r, payload); err != nil {
		return nil, fmt.Errorf("read frame payload: %w", err)
	}
	return payload, nil
}
