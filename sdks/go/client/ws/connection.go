package ws

import (
	"context"
	"fmt"
	"net/http"

	"github.com/coder/websocket"
)

// Connection wraps a WebSocket connection for SpacetimeDB protocol v2.
type Connection interface {
	Send(ctx context.Context, data []byte) error
	Recv(ctx context.Context) ([]byte, error)
	Close() error
}

// ConnectionBuilder builds a WebSocket connection.
type ConnectionBuilder interface {
	WithUri(uri string) ConnectionBuilder
	WithDatabaseName(name string) ConnectionBuilder
	WithToken(token string) ConnectionBuilder
	WithProtocol(protocol string) ConnectionBuilder
	Build(ctx context.Context) (Connection, error)
}

func NewConnection() ConnectionBuilder {
	return &connectionBuilder{
		protocol: "v2.bsatn.spacetimedb",
	}
}

type connectionBuilder struct {
	uri      string
	database string
	token    string
	protocol string
}

func (b *connectionBuilder) WithUri(uri string) ConnectionBuilder {
	b.uri = uri
	return b
}

func (b *connectionBuilder) WithDatabaseName(name string) ConnectionBuilder {
	b.database = name
	return b
}

func (b *connectionBuilder) WithToken(token string) ConnectionBuilder {
	b.token = token
	return b
}

func (b *connectionBuilder) WithProtocol(protocol string) ConnectionBuilder {
	b.protocol = protocol
	return b
}

func (b *connectionBuilder) Build(ctx context.Context) (Connection, error) {
	// Build WebSocket URL: ws://{host}/subscribe/{database}
	wsURL := fmt.Sprintf("%s/subscribe/%s", b.uri, b.database)

	// Set up headers
	headers := http.Header{}
	if b.token != "" {
		headers.Set("Authorization", fmt.Sprintf("Bearer %s", b.token))
	}

	// Dial with protocol negotiation
	conn, _, err := websocket.Dial(ctx, wsURL, &websocket.DialOptions{
		Subprotocols: []string{b.protocol},
		HTTPHeader:   headers,
	})
	if err != nil {
		return nil, fmt.Errorf("ws: dial failed: %w", err)
	}

	// Set reasonable read limit (33MB like Rust server)
	conn.SetReadLimit(33 * 1024 * 1024)

	return &connection{conn: conn}, nil
}

type connection struct {
	conn *websocket.Conn
}

func (c *connection) Send(ctx context.Context, data []byte) error {
	return c.conn.Write(ctx, websocket.MessageBinary, data)
}

func (c *connection) Recv(ctx context.Context) ([]byte, error) {
	_, data, err := c.conn.Read(ctx)
	if err != nil {
		return nil, err
	}
	return data, nil
}

func (c *connection) Close() error {
	return c.conn.Close(websocket.StatusNormalClosure, "client disconnect")
}
