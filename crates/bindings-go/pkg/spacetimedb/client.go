package spacetimedb

import (
	"context"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/db"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/wasm"
)

// Client represents a SpacetimeDB client instance
type Client struct {
	db   *db.Database
	wasm *wasm.Runtime
	ctx  context.Context
}

// NewClient creates a new SpacetimeDB client
func NewClient() (*Client, error) {
	database, err := db.NewDatabase()
	if err != nil {
		return nil, err
	}

	runtime := wasm.NewRuntime(wasm.DefaultConfig())
	if runtime == nil {
		database.Close()
		return nil, wasm.NewWASMError(wasm.ErrCodeRuntimeInit, "failed to initialize WASM runtime", nil)
	}

	return &Client{
		db:   database,
		wasm: runtime,
		ctx:  context.Background(),
	}, nil
}

// Close closes the client and releases all resources
func (c *Client) Close() error {
	if err := c.db.Close(); err != nil {
		return err
	}
	return c.wasm.Close(c.ctx)
}
