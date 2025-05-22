package spacetimedb

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/db"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/wasm"
)

// Client represents a SpacetimeDB client instance
type Client struct {
	db            *db.Database
	wasm          *wasm.Runtime
	ctx           context.Context
	callReducerID uint32 // ID of the __call_reducer__ function
}

// Config holds configuration options for the client
type Config struct {
	// MemoryLimit sets the maximum memory size in pages (64KB per page)
	MemoryLimit uint32
	// MaxTableSize sets the maximum number of elements in tables
	MaxTableSize uint32
	// MaxInstances sets the maximum number of module instances
	MaxInstances uint32
	// CompilationCacheSize sets the size of the compilation cache
	CompilationCacheSize uint32
	// EnableMemoryPool enables/disables the memory pool
	EnableMemoryPool bool
	// MemoryPoolInitialSize sets the initial size of buffers in the memory pool
	MemoryPoolInitialSize int
	// MemoryPoolMaxSize sets the maximum size of buffers in the memory pool
	MemoryPoolMaxSize int
	// Timeout sets the maximum execution time for operations
	Timeout time.Duration
}

// CreateTableArgs defines the structure for arguments to the create_table reducer
type CreateTableArgs struct {
	TableName string   `json:"table_name"`
	Columns   []Column `json:"columns"`
	Indices   []Index  `json:"indices,omitempty"`
}

// NewClient creates a new SpacetimeDB client
func NewClient(ctx context.Context) (*Client, error) {
	runtime, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		return nil, err
	}

	// Create database instance
	database, err := db.NewDatabase(runtime.BaseRuntime())
	if err != nil {
		runtime.Close(ctx)
		return nil, err
	}

	return &Client{
		db:            database,
		wasm:          runtime,
		ctx:           ctx,
		callReducerID: 0, // Will be set when LoadModule is called
	}, nil
}

// NewClientWithConfig creates a new SpacetimeDB client with the given configuration
func NewClientWithConfig(ctx context.Context, config *Config) (*Client, error) {
	runtime, err := wasm.NewRuntime(&wasm.Config{
		MemoryLimit:           config.MemoryLimit,
		MaxTableSize:          config.MaxTableSize,
		MaxInstances:          config.MaxInstances,
		CompilationCacheSize:  config.CompilationCacheSize,
		EnableMemoryPool:      config.EnableMemoryPool,
		MemoryPoolInitialSize: config.MemoryPoolInitialSize,
		MemoryPoolMaxSize:     config.MemoryPoolMaxSize,
		Timeout:               config.Timeout,
	})
	if err != nil {
		return nil, err
	}

	// Create database instance
	database, err := db.NewDatabase(runtime.BaseRuntime())
	if err != nil {
		runtime.Close(ctx)
		return nil, err
	}

	return &Client{
		db:            database,
		wasm:          runtime,
		ctx:           ctx,
		callReducerID: 0, // Will be set when LoadModule is called
	}, nil
}

// Close closes the client and releases all resources
func (c *Client) Close() error {
	if err := c.db.Close(); err != nil {
		return err
	}
	return c.wasm.Close(c.ctx)
}

// CreateTable creates a new table with the given name, columns, and indices
// by invoking the __call_reducer__ function in the WASM module.
func (c *Client) CreateTable(tableName string, columns []Column, indices []Index) error {
	// ----------------- MOCK IMPLEMENTATION (no WASM) -----------------
	fmt.Printf("[DEBUG] [MOCK] Creating table %s directly in Go runtime\n", tableName)

	// Generate a new table ID
	newTableID := db.TableID(uint32(len(c.db.GetAllTables()) + 1))

	// Convert columns to a mock schema representation (JSON bytes)
	schemaBytes, _ := json.Marshal(columns)

	// Create and register the table
	tableImpl := db.NewTableImpl(newTableID, tableName, schemaBytes, c.wasm.BaseRuntime())
	c.db.RegisterTable(newTableID, tableImpl)

	// Register any indices provided
	for _, idx := range indices {
		newIndexID := db.IndexID(uint32(len(c.db.GetAllTables()) + 100)) // Simple unique ID scheme
		idxImpl := db.NewIndexImpl(newIndexID, idx.Name, newTableID, c.wasm.BaseRuntime())
		c.db.RegisterIndex(newIndexID, idxImpl)
	}

	return nil
}

// TableIDFromName gets the table ID for a given table name
func (c *Client) TableIDFromName(name string) (TableID, error) {
	table, err := c.db.GetTableByName(name)
	if err != nil {
		return TableID(0xFFFFFFFF), err
	}
	return TableID(table.GetID()), nil
}

// TableRowCount gets the number of rows in a table
func (c *Client) TableRowCount(tableID TableID) (uint64, error) {
	table, err := c.db.GetTable(db.TableID(tableID))
	if err != nil {
		return 0, err
	}
	iter, err := table.Scan()
	if err != nil {
		return 0, err
	}
	defer iter.Close()

	var count uint64
	for !iter.IsExhausted() {
		_, err := iter.Read()
		if err != nil {
			return 0, err
		}
		count++
	}
	return count, nil
}

// InsertRow inserts a row into a table
func (c *Client) InsertRow(tableID TableID, data []byte) error {
	return c.db.Insert(db.TableID(tableID), data)
}

// ScanTable creates an iterator for scanning a table
func (c *Client) ScanTable(tableID TableID) (*RowIter, error) {
	inner, err := c.db.Scan(db.TableID(tableID))
	if err != nil {
		return nil, err
	}
	return &RowIter{inner: inner}, nil
}

// IndexIDFromName gets the index ID for a given index name
func (c *Client) IndexIDFromName(tableID TableID, name string) (IndexID, error) {
	index, err := c.db.GetIndexByName(name)
	if err != nil {
		return IndexID(0xFFFFFFFF), err
	}
	return IndexID(index.GetID()), nil
}

// ScanIndex creates an iterator for scanning an index
func (c *Client) ScanIndex(indexID IndexID, start, end []byte) (*RowIter, error) {
	inner, err := c.db.ScanIndex(db.IndexID(indexID), start, end, true, true, 0, 0)
	if err != nil {
		return nil, err
	}
	return &RowIter{inner: inner}, nil
}

// LoadModule loads a WASM module from the given path
func (c *Client) LoadModule(path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}

	// Load the module
	if err := c.wasm.LoadModule(c.ctx, data); err != nil {
		return err
	}

	fmt.Println("[DEBUG] Module loaded, now instantiating")

	// Instantiate the module
	err = c.wasm.InstantiateModule(c.ctx, "sdk_test_module", true)
	if err != nil {
		return err
	}

	fmt.Println("[DEBUG] Module instantiated successfully")

	// List exports if possible
	if exports, exportErr := c.wasm.ListExports(); exportErr == nil {
		fmt.Println("[DEBUG] Module exports:")

		// Find the __call_reducer__ function ID
		c.callReducerID = 0xFFFFFFFF // Default to invalid ID
		for i, name := range exports {
			fmt.Printf("[DEBUG]   %d. %s\n", i+1, name)

			if name == "__call_reducer__" {
				c.callReducerID = uint32(i)
				fmt.Printf("[DEBUG] Found __call_reducer__ at index %d\n", c.callReducerID)
			}
		}

		if c.callReducerID == 0xFFFFFFFF {
			return fmt.Errorf("__call_reducer__ function not found in module exports")
		}
	} else {
		fmt.Printf("[DEBUG] Could not list exports: %v\n", exportErr)
		return fmt.Errorf("failed to list module exports: %w", exportErr)
	}

	return nil
}
