package tests

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCoreTypesIntegration(t *testing.T) {
	// Initialize test environment
	ctx := setupTestEnvironment(t)
	defer ctx.cleanup()

	// Create test tables
	err := ctx.client.CreateTable("one_u8", []spacetimedb.Column{
		{Name: "n", Type: "u8"},
	}, nil)
	require.NoError(t, err)

	err = ctx.client.CreateTable("unique_u8", []spacetimedb.Column{
		{Name: "n", Type: "u8"},
	}, []spacetimedb.Index{
		{Name: "n", Columns: []string{"n"}, Unique: true},
	})
	require.NoError(t, err)

	// Test basic table operations
	t.Run("BasicTableOperations", func(t *testing.T) {
		// Test table creation and validation
		tableID, err := ctx.client.TableIDFromName("one_u8")
		require.NoError(t, err)
		assert.NotEqual(t, spacetimedb.TableID(0xFFFFFFFF), tableID)

		// Test row count
		count, err := ctx.client.TableRowCount(tableID)
		require.NoError(t, err)
		assert.Equal(t, uint64(0), count)

		// Test row insertion
		data := []byte{0x01} // BSATN encoded uint8 value
		err = ctx.client.InsertRow(tableID, data)
		require.NoError(t, err)

		// Verify row count after insertion
		count, err = ctx.client.TableRowCount(tableID)
		require.NoError(t, err)
		assert.Equal(t, uint64(1), count)

		// Test row iteration
		iter, err := ctx.client.ScanTable(tableID)
		require.NoError(t, err)
		defer iter.Close()

		// Read the row
		row, err := iter.Read()
		require.NoError(t, err)
		assert.Equal(t, data, row)

		// Verify iterator is exhausted
		row, err = iter.Read()
		assert.Error(t, err)
		assert.True(t, iter.IsExhausted())
	})

	// Test index operations
	t.Run("IndexOperations", func(t *testing.T) {
		// Test index creation and validation
		tableID, err := ctx.client.TableIDFromName("unique_u8")
		require.NoError(t, err)
		assert.NotEqual(t, spacetimedb.TableID(0xFFFFFFFF), tableID)

		indexID, err := ctx.client.IndexIDFromName(tableID, "n")
		require.NoError(t, err)
		assert.NotEqual(t, spacetimedb.IndexID(0xFFFFFFFF), indexID)

		// Test index scanning
		iter, err := ctx.client.ScanIndex(indexID, []byte{0x01}, []byte{0x02})
		require.NoError(t, err)
		defer iter.Close()

		// Verify iterator is exhausted (no rows yet)
		assert.True(t, iter.IsExhausted())
	})
}

// TestContext holds the test environment
type TestContext struct {
	client  *spacetimedb.Client
	cleanup func()
}

// setupTestEnvironment initializes the test environment
func setupTestEnvironment(t *testing.T) *TestContext {
	// Create client with custom config
	config := &spacetimedb.Config{
		MemoryLimit:           2000, // ~128MB
		MaxTableSize:          1000,
		MaxInstances:          100,
		CompilationCacheSize:  100,
		EnableMemoryPool:      true,
		MemoryPoolInitialSize: 8192,   // 8KB
		MemoryPoolMaxSize:     204800, // 200KB
		Timeout:               time.Second * 60,
	}

	// Enable wazero debug logging if supported
	if debugEnv := os.Getenv("WAZERO_DEBUG"); debugEnv == "1" {
		fmt.Println("[DEBUG] Enabling wazero debug logging")
	}

	client, err := spacetimedb.NewClientWithConfig(context.Background(), config)
	require.NoError(t, err)

	// Get the workspace root
	workspaceRoot := os.Getenv("SPACETIMEDB_DIR")
	if workspaceRoot == "" {
		t.Fatal("SPACETIMEDB_DIR environment variable not set")
	}

	// Load the WASM module
	wasmPath := filepath.Join(
		workspaceRoot,
		"target/wasm32-unknown-unknown/release/sdk_test_module.wasm",
	)

	// Check if the module exists
	if _, err := os.Stat(wasmPath); os.IsNotExist(err) {
		t.Fatalf("WASM module not found at path: %s", wasmPath)
	}

	// Load the module with detailed error handling
	err = client.LoadModule(wasmPath)
	if err != nil {
		fmt.Printf("[DEBUG] WASM instantiation error: %+v\n", err)
		t.Fatalf("Failed to load WASM module: %v\nModule path: %s", err, wasmPath)
	}

	// Set up cleanup function
	cleanup := func() {
		client.Close()
	}

	return &TestContext{
		client:  client,
		cleanup: cleanup,
	}
}
