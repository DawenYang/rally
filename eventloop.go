package rally

import (
	"context"
	"net"
)

type EventLoop interface {
	Serve(ln net.Listener) error
	Shutdown(ctx context.Context) error
}

type OnPrepare func(connection Connection)

type OnConnect func(ctx context.Context, connection Connection) context.Context

type OnDisconnect func(ctx context.Context, connection Connection)

type OnRequest func(ctx context.Context, connection Connection) error
