package wasm_test

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/wasm"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createTestModule creates a WASM module by reading from fixtures/add.wasm
func createTestModule() []byte {
	wasmBytes, err := os.ReadFile("fixtures/add.wasm")
	if err != nil {
		panic(fmt.Sprintf("failed to read fixtures/add.wasm: %v", err))
	}
	return wasmBytes
}

// TestRuntime_NewRuntime tests the creation of a new runtime
func TestRuntime_NewRuntime(t *testing.T) {
	tests := []struct {
		name   string
		config *wasm.Config
		want   *wasm.Config
	}{
		{
			name:   "nil config uses defaults",
			config: nil,
			want:   wasm.DefaultConfig(),
		},
		{
			name: "custom config",
			config: &wasm.Config{
				MemoryLimit:           2000,
				MaxTableSize:          2000,
				MaxInstances:          200,
				CompilationCacheSize:  200,
				EnableMemoryPool:      false,
				MemoryPoolInitialSize: 8192,
				MemoryPoolMaxSize:     204800,
				Timeout:               time.Second * 60,
			},
			want: &wasm.Config{
				MemoryLimit:           2000,
				MaxTableSize:          2000,
				MaxInstances:          200,
				CompilationCacheSize:  200,
				EnableMemoryPool:      false,
				MemoryPoolInitialSize: 8192,
				MemoryPoolMaxSize:     204800,
				Timeout:               time.Second * 60,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(tt.config)
			assert.Equal(t, tt.want, r.Config)
		})
	}
}

// TestRuntime_LoadModule tests loading and compiling a WASM module
func TestRuntime_LoadModule(t *testing.T) {
	tests := []struct {
		name      string
		wasmBytes []byte
		wantErr   bool
		errCode   uint16
	}{
		{
			name:      "valid module",
			wasmBytes: createTestModule(),
			wantErr:   false,
		},
		{
			name:      "invalid module",
			wasmBytes: []byte{0x00, 0x00},
			wantErr:   true,
			errCode:   wasm.ErrCodeCompileFailed,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			err := r.LoadModule(context.Background(), tt.wasmBytes)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, r.Module)
			}
		})
	}
}

// TestRuntime_InstantiateModule tests instantiating a WASM module
func TestRuntime_InstantiateModule(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		wantErr bool
		errCode uint16
	}{
		{
			name: "no module loaded",
			setup: func(r *wasm.Runtime) error {
				// Don't load any module
				return nil
			},
			wantErr: true,
			errCode: wasm.ErrCodeNoModuleLoaded,
		},
		{
			name: "valid module",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				return nil
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			err = r.InstantiateModule(context.Background(), "", true)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

// TestRuntime_CallFunction tests calling functions in a WASM module
func TestRuntime_CallFunction(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		fnName  string
		params  []interface{}
		want    []uint64
		wantErr bool
		errCode uint16
	}{
		{
			name: "no module instantiated",
			setup: func(r *wasm.Runtime) error {
				// Don't load or instantiate any module
				return nil
			},
			fnName:  "test",
			wantErr: true,
			errCode: wasm.ErrCodeNoModuleLoaded,
		},
		{
			name: "function not found",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background(), "", true); err != nil {
					return err
				}
				return nil
			},
			fnName:  "nonexistent",
			wantErr: true,
			errCode: wasm.ErrCodeFunctionNotFound,
		},
		{
			name: "successful function call",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background(), "", true); err != nil {
					return err
				}
				return nil
			},
			fnName:  "add",
			params:  []interface{}{int32(1), int32(2)},
			want:    []uint64{3},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			got, err := r.CallFunction(context.Background(), tt.fnName, tt.params...)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.want, got)
			}
		})
	}
}

// TestRuntime_MemoryManagement tests memory management functionality
func TestRuntime_MemoryManagement(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		data    []byte
		wantErr bool
		errCode uint16
	}{
		{
			name: "no memory available",
			setup: func(r *wasm.Runtime) error {
				// Don't instantiate any module
				return nil
			},
			data:    []byte{1, 2, 3},
			wantErr: true,
			errCode: wasm.ErrCodeNoMemory,
		},
		{
			name: "memory limit exceeded",
			setup: func(r *wasm.Runtime) error {
				// Setup module with small memory limit
				r.Config.MemoryLimit = 2 // 128KB
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background(), "", true); err != nil {
					return err
				}
				return nil
			},
			data:    make([]byte, 129537), // Exceed 128KB
			wantErr: true,
			errCode: wasm.ErrCodeMemoryExceeded,
		},
		{
			name: "successful memory write and read",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background(), "", true); err != nil {
					return err
				}
				return nil
			},
			data:    []byte{1, 2, 3, 4, 5},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			ptr, err := r.WriteToMemory(tt.data)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
				// Read back the data
				data, err := r.ReadFromMemory(ptr, uint32(len(tt.data)))
				assert.NoError(t, err)
				assert.Equal(t, tt.data, data)
			}
		})
	}
}

// TestRuntime_BufferPool tests the buffer pool functionality
func TestRuntime_BufferPool(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*wasm.Runtime)
		size    int
		wantErr bool
		errCode uint16
	}{
		{
			name: "pool disabled",
			setup: func(r *wasm.Runtime) {
				r.Config.EnableMemoryPool = false
			},
			size:    4096,
			wantErr: false,
		},
		{
			name: "buffer too large",
			setup: func(r *wasm.Runtime) {
				r.Config.EnableMemoryPool = true
				r.Config.MemoryPoolMaxSize = 1024
			},
			size:    2048,
			wantErr: true,
			errCode: wasm.ErrCodePoolPutFailed,
		},
		// Add more test cases for successful buffer operations
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			tt.setup(r)
			buf := r.GetBuffer()
			assert.NotNil(t, buf)
			// Write some data
			buf.Write(make([]byte, tt.size))
			// Return buffer to pool
			err := r.PutBuffer(buf)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

// TestRuntime_Close tests the cleanup functionality
func TestRuntime_Close(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*wasm.Runtime)
		wantErr bool
		errCode uint16
	}{
		{
			name: "cleanup function fails",
			setup: func(r *wasm.Runtime) {
				r.AddCleanup(func() error {
					return assert.AnError
				})
			},
			wantErr: true,
			errCode: wasm.ErrCodeCleanupFailed,
		},
		{
			name: "successful cleanup",
			setup: func(r *wasm.Runtime) {
				// Add successful cleanup function
				r.AddCleanup(func() error {
					return nil
				})
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			tt.setup(r)
			err := r.Close(context.Background())
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

// TestRuntime_GetMemoryStats tests memory statistics functionality
func TestRuntime_GetMemoryStats(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		wantErr bool
		errCode uint16
	}{
		{
			name: "no memory available",
			setup: func(r *wasm.Runtime) error {
				// Don't instantiate any module
				return nil
			},
			wantErr: true,
			errCode: wasm.ErrCodeNoMemory,
		},
		{
			name: "valid memory stats",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background(), "", true); err != nil {
					return err
				}
				return nil
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := wasm.NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			stats, err := r.GetMemoryStats()
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*wasm.WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, stats)
				// Verify stats fields
				assert.GreaterOrEqual(t, stats.Usage, uint64(0))
				assert.GreaterOrEqual(t, stats.Allocs, uint64(0))
				assert.GreaterOrEqual(t, stats.Frees, uint64(0))
				assert.GreaterOrEqual(t, stats.Size, uint32(0))
				assert.GreaterOrEqual(t, stats.Capacity, uint32(0))
			}
		})
	}
}

// TestRuntime_DebugInstantiate tests instantiating a WASM module with debug information
func TestRuntime_DebugInstantiate(t *testing.T) {
	// Create runtime with debug config
	config := wasm.DefaultConfig()
	config.EnableMemoryPool = false // Disable memory pool for simpler debugging
	r := wasm.NewRuntime(config)

	// Load the module
	wasmBytes := createTestModule()
	err := r.LoadModule(context.Background(), wasmBytes)
	require.NoError(t, err)

	// Print module information
	fmt.Printf("\nModule Information:\n")
	fmt.Printf("Exported Functions:\n")
	for _, exp := range r.Module.ExportedFunctions() {
		fmt.Printf("  Export: %s\n", exp.Name())
	}
	fmt.Printf("Imported Functions:\n")
	for _, imp := range r.Module.ImportedFunctions() {
		fmt.Printf("  Import: %s\n", imp.Name())
	}

	// Try instantiation without WASI first
	fmt.Printf("\nTrying instantiation without WASI...\n")
	err = r.InstantiateModule(context.Background(), "test_module", false)
	if err != nil {
		fmt.Printf("Instantiation without WASI failed: %v\n", err)
	} else {
		fmt.Printf("Instantiation without WASI succeeded\n")
	}

	// Close the runtime
	err = r.Close(context.Background())
	require.NoError(t, err)

	// Create a new runtime for WASI test
	r = wasm.NewRuntime(config)
	err = r.LoadModule(context.Background(), wasmBytes)
	require.NoError(t, err)

	// Try instantiation with WASI
	fmt.Printf("\nTrying instantiation with WASI...\n")
	err = r.InstantiateModule(context.Background(), "test_module", true)
	if err != nil {
		fmt.Printf("Instantiation with WASI failed: %v\n", err)
	} else {
		fmt.Printf("Instantiation with WASI succeeded\n")
	}

	// Close the runtime
	err = r.Close(context.Background())
	require.NoError(t, err)
}

// TestSimpleAddFunction tests the add function in a simple C-based WASM module
func TestSimpleAddFunction(t *testing.T) {
	// Create runtime with default config
	r := wasm.NewRuntime(nil)
	defer r.Close(context.Background())

	// Load the WASM module
	wasmBytes := createTestModule()
	err := r.LoadModule(context.Background(), wasmBytes)
	require.NoError(t, err)

	// Instantiate the module (no WASI support needed for simple C module)
	err = r.InstantiateModule(context.Background(), "add_module", false)
	require.NoError(t, err)

	testCases := []struct {
		name     string
		a, b     int32
		expected int32
	}{
		{"positive_numbers", 5, 3, 8},
		{"zero", 0, 0, 0},
		{"negative_numbers", -5, -3, -8},
		{"mixed_numbers", 5, -3, 2},
		{"large_numbers", 1000000, 2000000, 3000000},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			// Call the add function directly (no .command_export needed for C module)
			results, err := r.CallFunction(context.Background(), "add", tc.a, tc.b)
			require.NoError(t, err)
			require.Len(t, results, 1, "Expected one result")
			sum := int32(results[0])
			assert.Equal(t, tc.expected, sum)
		})
	}
}

// TestSimpleAddFunctionError tests error cases for the add function in a simple C-based WASM module
func TestSimpleAddFunctionError(t *testing.T) {
	// Create runtime with default config
	r := wasm.NewRuntime(nil)
	defer r.Close(context.Background())

	// Load the WASM module
	wasmBytes := createTestModule()
	err := r.LoadModule(context.Background(), wasmBytes)
	require.NoError(t, err)

	// Instantiate the module (no WASI support needed for simple C module)
	err = r.InstantiateModule(context.Background(), "add_module", false)
	require.NoError(t, err)

	testCases := []struct {
		name        string
		args        []interface{}
		expectError bool
	}{
		{"too_few_parameters", []interface{}{1}, true},
		{"too_many_parameters", []interface{}{1, 2, 3}, true},
		{"wrong_parameter_type", []interface{}{"1", 2}, true},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			_, err := r.CallFunction(context.Background(), "add", tc.args...)
			if tc.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}
