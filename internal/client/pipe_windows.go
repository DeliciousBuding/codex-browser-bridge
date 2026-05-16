package client

import (
	"net"
	"time"

	"github.com/Microsoft/go-winio"
)

func dialNamedPipe(path string) (net.Conn, error) {
	timeout := 5 * time.Second
	conn, err := winio.DialPipe(path, &timeout)
	if err != nil {
		return nil, err
	}
	return conn, nil
}
