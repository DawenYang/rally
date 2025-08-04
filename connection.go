package rally

import (
	"net"
	"time"
)

type CloseCallback func(connection Connection)

type Connection interface {
	net.Conn
	Reader() Reader
	Writer() Writer
	IsActive() bool
	SetReadTimeout(timeout time.Duration) error
	SetWriteTimeout(timeout time.Duration) error
	SetIdleTimeout(timeout time.Duration) error
	SetOnRequest(on OnRequest) error
}
