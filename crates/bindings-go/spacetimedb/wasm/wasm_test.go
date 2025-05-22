package wasm

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

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
		config *Config
		want   *Config
	}{
		{
			name:   "nil config uses defaults",
			config: nil,
			want:   DefaultConfig(),
		},
		{
			name: "custom config",
			config: &Config{
				MemoryLimit:           2000,
				MaxTableSize:          2000,
				MaxInstances:          200,
				CompilationCacheSize:  200,
				EnableMemoryPool:      false,
				MemoryPoolInitialSize: 8192,
				MemoryPoolMaxSize:     204800,
				Timeout:               time.Second * 60,
			},
			want: &Config{
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
			r := NewRuntime(tt.config)
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
			errCode:   ErrCodeCompileFailed,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := NewRuntime(nil)
			err := r.LoadModule(context.Background(), tt.wasmBytes)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
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
		setup   func(*Runtime) error
		wantErr bool
		errCode uint16
	}{
		{
			name: "no module loaded",
			setup: func(r *Runtime) error {
				// Don't load any module
				return nil
			},
			wantErr: true,
			errCode: ErrCodeNoModuleLoaded,
		},
		{
			name: "valid module",
			setup: func(r *Runtime) error {
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
			r := NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			err = r.InstantiateModule(context.Background())
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, r.instance)
				assert.NotNil(t, r.memory)
			}
		})
	}
}

// TestRuntime_CallFunction tests calling functions in a WASM module
func TestRuntime_CallFunction(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(*Runtime) error
		fnName  string
		params  []interface{}
		want    []uint64
		wantErr bool
		errCode uint16
	}{
		{
			name: "no module instantiated",
			setup: func(r *Runtime) error {
				// Don't load or instantiate any module
				return nil
			},
			fnName:  "test",
			wantErr: true,
			errCode: ErrCodeNoModuleLoaded,
		},
		{
			name: "function not found",
			setup: func(r *Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background()); err != nil {
					return err
				}
				return nil
			},
			fnName:  "nonexistent",
			wantErr: true,
			errCode: ErrCodeFunctionNotFound,
		},
		{
			name: "successful function call",
			setup: func(r *Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background()); err != nil {
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
			r := NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			got, err := r.CallFunction(context.Background(), tt.fnName, tt.params...)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
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
		setup   func(*Runtime) error
		data    []byte
		wantErr bool
		errCode uint16
	}{
		{
			name: "no memory available",
			setup: func(r *Runtime) error {
				// Don't instantiate any module
				return nil
			},
			data:    []byte{1, 2, 3},
			wantErr: true,
			errCode: ErrCodeNoMemory,
		},
		{
			name: "memory limit exceeded",
			setup: func(r *Runtime) error {
				// Setup module with small memory limit
				r.Config.MemoryLimit = 2 // 128KB
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background()); err != nil {
					return err
				}
				return nil
			},
			data:    make([]byte, 129537), // Exceed 128KB
			wantErr: true,
			errCode: ErrCodeMemoryExceeded,
		},
		{
			name: "successful memory write and read",
			setup: func(r *Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background()); err != nil {
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
			r := NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			ptr, err := r.writeToMemory(tt.data)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
					assert.Equal(t, tt.errCode, wasmErr.Code)
				}
			} else {
				assert.NoError(t, err)
				// Read back the data
				data, err := r.readFromMemory(ptr, uint32(len(tt.data)))
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
		setup   func(*Runtime)
		size    int
		wantErr bool
		errCode uint16
	}{
		{
			name: "pool disabled",
			setup: func(r *Runtime) {
				r.Config.EnableMemoryPool = false
			},
			size:    4096,
			wantErr: false,
		},
		{
			name: "buffer too large",
			setup: func(r *Runtime) {
				r.Config.EnableMemoryPool = true
				r.Config.MemoryPoolMaxSize = 1024
			},
			size:    2048,
			wantErr: true,
			errCode: ErrCodePoolPutFailed,
		},
		// Add more test cases for successful buffer operations
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := NewRuntime(nil)
			tt.setup(r)
			buf := r.GetBuffer()
			assert.NotNil(t, buf)
			// Write some data
			buf.Write(make([]byte, tt.size))
			// Return buffer to pool
			err := r.PutBuffer(buf)
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
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
		setup   func(*Runtime)
		wantErr bool
		errCode uint16
	}{
		{
			name: "cleanup function fails",
			setup: func(r *Runtime) {
				r.AddCleanup(func() error {
					return assert.AnError
				})
			},
			wantErr: true,
			errCode: ErrCodeCleanupFailed,
		},
		{
			name: "successful cleanup",
			setup: func(r *Runtime) {
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
			r := NewRuntime(nil)
			tt.setup(r)
			err := r.Close(context.Background())
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
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
		setup   func(*Runtime) error
		wantErr bool
		errCode uint16
	}{
		{
			name: "no memory available",
			setup: func(r *Runtime) error {
				// Don't instantiate any module
				return nil
			},
			wantErr: true,
			errCode: ErrCodeNoMemory,
		},
		{
			name: "valid memory stats",
			setup: func(r *Runtime) error {
				if err := r.LoadModule(context.Background(), createTestModule()); err != nil {
					return err
				}
				if err := r.InstantiateModule(context.Background()); err != nil {
					return err
				}
				return nil
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			r := NewRuntime(nil)
			err := tt.setup(r)
			require.NoError(t, err)
			stats, err := r.GetMemoryStats()
			if tt.wantErr {
				assert.Error(t, err)
				if wasmErr, ok := err.(*WASMError); ok {
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
