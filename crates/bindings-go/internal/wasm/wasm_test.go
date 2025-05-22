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

// createTestModule creates a WASM module by reading from fixtures/get_buffer_ptr.wasm
func createTestModule() []byte {
	wasmBytes, err := os.ReadFile("fixtures/get_buffer_ptr.wasm")
	if err != nil {
		panic(fmt.Sprintf("failed to read fixtures/get_buffer_ptr.wasm: %v", err))
	}
	return wasmBytes
}

// createAddModule creates a WASM module by reading from fixtures/add.wasm
func createAddModule() []byte {
	wasmBytes, err := os.ReadFile("fixtures/add.wasm")
	if err != nil {
		panic(fmt.Sprintf("failed to read fixtures/add.wasm: %v", err))
	}
	return wasmBytes
}

// createBufferPtrModule creates a WASM module by reading from fixtures/get_buffer_ptr.wasm
func createBufferPtrModule() []byte {
	wasmBytes, err := os.ReadFile("fixtures/get_buffer_ptr.wasm")
	if err != nil {
		panic(fmt.Sprintf("failed to read fixtures/get_buffer_ptr.wasm: %v", err))
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
			r, err := wasm.NewRuntime(tt.config)
			if err != nil {
				t.Fatalf("failed to create runtime: %v", err)
			}
			assert.Equal(t, tt.want, r.Config)
		})
	}
}

// TestRuntime_LoadModule tests loading and compiling a WASM module
func TestRuntime_LoadModule(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	tests := []struct {
		name      string
		wasmBytes []byte
		wantErr   bool
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
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := r.LoadModule(context.Background(), tt.wasmBytes)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, r.Module)
			}
		})
	}
}

// TestRuntime_InstantiateModule tests instantiating a WASM module
func TestRuntime_InstantiateModule(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		wantErr bool
	}{
		{
			name: "valid module",
			setup: func(r *wasm.Runtime) error {
				return r.LoadModule(context.Background(), createTestModule())
			},
			wantErr: false,
		},
		{
			name: "invalid module",
			setup: func(r *wasm.Runtime) error {
				return r.LoadModule(context.Background(), []byte{0x00})
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.setup(r)
			if tt.wantErr {
				assert.Error(t, err)
				return
			}
			require.NoError(t, err)
			err = r.InstantiateModule(context.Background(), "", true)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

// TestRuntime_CallFunction tests calling functions in a WASM module
func TestRuntime_CallFunction(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		fnName  string
		params  []interface{}
		want    []uint64
		wantErr bool
	}{
		{
			name: "valid function",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createAddModule()); err != nil {
					return err
				}
				return r.InstantiateModule(context.Background(), "test_module", false)
			},
			fnName:  "add",
			params:  []interface{}{int32(1), int32(2)},
			want:    []uint64{3},
			wantErr: false,
		},
		{
			name: "invalid function",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), []byte{0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00}); err != nil {
					return err
				}
				return r.InstantiateModule(context.Background(), "test_module", false)
			},
			fnName:  "nonexistent",
			params:  []interface{}{},
			want:    nil,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.setup(r)
			if tt.wantErr {
				assert.Error(t, err)
				return
			}
			require.NoError(t, err)
			got, err := r.CallFunction(context.Background(), tt.fnName, tt.params...)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.want, got)
			}
		})
	}
}

// TestRuntime_MemoryManagement tests memory management functionality
func TestRuntime_MemoryManagement(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		data    []byte
		wantErr bool
	}{
		{
			name: "valid data",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), createBufferPtrModule()); err != nil {
					return err
				}
				return r.InstantiateModule(context.Background(), "test_module", false)
			},
			data:    []byte{1, 2, 3, 4},
			wantErr: false,
		},
		{
			name: "empty data",
			setup: func(r *wasm.Runtime) error {
				if err := r.LoadModule(context.Background(), []byte{0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00}); err != nil {
					return err
				}
				return r.InstantiateModule(context.Background(), "test_module", false)
			},
			data:    []byte{},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.setup(r)
			if tt.wantErr && err != nil {
				assert.Error(t, err)
				return
			}
			require.NoError(t, err)

			// Call the exported get_buffer_ptr function to get a valid pointer
			results, err := r.CallFunction(context.Background(), "get_buffer_ptr")
			require.NoError(t, err)
			require.Len(t, results, 1, "Expected one result from get_buffer_ptr")
			ptr := uint32(results[0])

			// Write data at the returned pointer
			err = r.WriteToMemoryAt(ptr, tt.data)
			if tt.wantErr {
				assert.Error(t, err)
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
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	tests := []struct {
		name    string
		setup   func(*wasm.Runtime) error
		data    []byte
		wantErr bool
	}{
		{
			name: "valid buffer",
			setup: func(r *wasm.Runtime) error {
				return r.LoadModule(context.Background(), createTestModule())
			},
			data:    []byte{1, 2, 3, 4},
			wantErr: false,
		},
		{
			name: "empty buffer",
			setup: func(r *wasm.Runtime) error {
				return r.LoadModule(context.Background(), []byte{0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00})
			},
			data:    []byte{},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.setup(r)
			require.NoError(t, err)
			buf := r.GetBuffer()
			_, err = buf.Write(tt.data)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.data, buf.Bytes())
				err = r.PutBuffer(buf)
				assert.NoError(t, err)
			}
		})
	}
}

// TestRuntime_Close tests closing the runtime
func TestRuntime_Close(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	err = r.Close(context.Background())
	assert.NoError(t, err)
}

// TestRuntime_AddCleanup tests adding cleanup functions
func TestRuntime_AddCleanup(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	cleanupCalled := false
	r.AddCleanup(func() error {
		cleanupCalled = true
		return nil
	})

	err = r.Close(context.Background())
	assert.NoError(t, err)
	assert.True(t, cleanupCalled)
}

// TestRuntime_GetMemoryStats tests getting memory statistics
func TestRuntime_GetMemoryStats(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	// Load and instantiate a test module
	err = r.LoadModule(context.Background(), createTestModule())
	require.NoError(t, err)

	err = r.InstantiateModule(context.Background(), "test", false)
	require.NoError(t, err)

	stats, err := r.GetMemoryStats()
	require.NoError(t, err)
	assert.NotNil(t, stats)
	assert.Equal(t, uint64(0), stats.Usage)
	assert.Equal(t, uint64(0), stats.Allocs)
	assert.Equal(t, uint64(0), stats.Frees)
}

// TestRuntime_UnmarshalResults tests unmarshaling function results
func TestRuntime_UnmarshalResults(t *testing.T) {
	r, err := wasm.NewRuntime(wasm.DefaultConfig())
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	tests := []struct {
		name    string
		results []uint64
		want    []interface{}
		wantErr bool
	}{
		{
			name:    "valid results",
			results: []uint64{1, 2, 3},
			want:    []interface{}{uint64(1), uint64(2), uint64(3)},
			wantErr: false,
		},
		{
			name:    "empty results",
			results: []uint64{},
			want:    []interface{}{},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := r.UnmarshalResults(tt.results)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.want, got)
			}
		})
	}
}

// TestRuntime_DebugInstantiate tests instantiating a WASM module with debug information
func TestRuntime_DebugInstantiate(t *testing.T) {
	// Create runtime with debug config
	config := wasm.DefaultConfig()
	config.EnableMemoryPool = false // Disable memory pool for simpler debugging
	r, err := wasm.NewRuntime(config)
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}

	// Load the module
	wasmBytes := createTestModule()
	err = r.LoadModule(context.Background(), wasmBytes)
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
	r, err = wasm.NewRuntime(config)
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
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
	r, err := wasm.NewRuntime(nil)
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	// Load the WASM module
	wasmBytes := createAddModule()
	err = r.LoadModule(context.Background(), wasmBytes)
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
	r, err := wasm.NewRuntime(nil)
	if err != nil {
		t.Fatalf("failed to create runtime: %v", err)
	}
	defer r.Close(context.Background())

	// Load the WASM module
	wasmBytes := createAddModule()
	err = r.LoadModule(context.Background(), wasmBytes)
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
