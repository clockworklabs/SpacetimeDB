package wasm

import (
	"bytes"
	"context"
	"fmt"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
	"math"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"runtime/debug"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

// Error codes
const (
	ErrCodeCompileFailed     = 1
	ErrCodeNoModuleLoaded    = 2
	ErrCodeInstantiateFailed = 3
	ErrCodeNoMemory          = 4
	ErrCodeOutOfBounds       = 5
	ErrCodeWriteFailed       = 6
	ErrCodeNoImports         = 7
	ErrCodeNoExports         = 8
	ErrCodeFunctionNotFound  = 9
	ErrCodeCallFailed        = 10
	ErrCodePanic             = 11
	ErrCodeTimeout           = 12
	ErrCodeContextCanceled   = 13
	ErrCodeRuntimeInit       = 14
	ErrCodeCloseFailed       = 15
	ErrCodeCleanupFailed     = 16
	ErrCodePoolGetFailed     = 17
	ErrCodePoolPutFailed     = 18
	ErrCodeUnsupportedType   = 19
	ErrCodeMemoryNotInit     = 20
	ErrCodeMemoryExceeded    = 21
	ErrCodeMemoryReadError   = 22
)

// WASMError represents a WASM-specific error
type WASMError struct {
	Code    uint16
	Message string
	Context map[string]interface{}
	Stack   string
}

func (e *WASMError) Error() string {
	return fmt.Sprintf("WASM error %d: %s", e.Code, e.Message)
}

// Unwrap returns the underlying error if any
func (e *WASMError) Unwrap() error {
	if err, ok := e.Context["error"].(error); ok {
		return err
	}
	return nil
}

// NewWASMError creates a new WASM error with stack trace
func NewWASMError(code uint16, message string, context map[string]interface{}) *WASMError {
	const depth = 32
	var pcs [depth]uintptr
	n := runtime.Callers(3, pcs[:])
	frames := runtime.CallersFrames(pcs[:n])

	var stack string
	for {
		frame, more := frames.Next()
		stack += fmt.Sprintf("\n%s\n\t%s:%d", frame.Function, frame.File, frame.Line)
		if !more {
			break
		}
	}

	return &WASMError{
		Code:    code,
		Message: message,
		Context: context,
		Stack:   stack,
	}
}

// Config holds configuration options for the WASM runtime
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

// DefaultConfig returns a Config with sensible defaults
func DefaultConfig() *Config {
	return &Config{
		MemoryLimit:           1000, // ~64MB
		MaxTableSize:          1000,
		MaxInstances:          100,
		CompilationCacheSize:  100,
		EnableMemoryPool:      true,
		MemoryPoolInitialSize: 4096,   // 4KB
		MemoryPoolMaxSize:     102400, // 100KB
		Timeout:               time.Second * 30,
	}
}

// MemoryStats holds memory usage statistics
type MemoryStats struct {
	Usage    uint64 // Current memory usage in bytes
	Allocs   uint64 // Number of allocations
	Frees    uint64 // Number of deallocations
	Size     uint32 // Current memory size in bytes
	Capacity uint32 // Maximum memory capacity in bytes
}

// Runtime represents a WASM runtime instance for SpacetimeDB
type Runtime struct {
	Runtime wazero.Runtime
	Module  wazero.CompiledModule
	// MemoryPool provides reusable buffers for WASM operations
	MemoryPool sync.Pool
	// Config holds the runtime configuration
	Config *Config
	// instance is the current module instance
	instance api.Module
	// memory is the current module's memory
	memory api.Memory
	// mu protects concurrent access to instance and memory
	mu sync.RWMutex
	// memoryUsage tracks current memory usage in bytes
	memoryUsage atomic.Uint64
	// memoryAllocs tracks number of memory allocations
	memoryAllocs atomic.Uint64
	// memoryFrees tracks number of memory deallocations
	memoryFrees atomic.Uint64
	// cleanupFuncs holds cleanup functions to be called on Close
	cleanupFuncs []func() error
}

// NewRuntime creates a new Runtime instance with the given configuration
func NewRuntime(config *Config) *Runtime {
	if config == nil {
		config = DefaultConfig()
	}

	r := &Runtime{
		Config: config,
	}

	if config.EnableMemoryPool {
		r.MemoryPool = sync.Pool{
			New: func() interface{} {
				return make([]byte, 0, config.MemoryPoolInitialSize)
			},
		}
	}

	return r
}

// LoadModule loads and compiles a WASM module
func (r *Runtime) LoadModule(ctx context.Context, wasmBytes []byte) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	// Create runtime config
	config := wazero.NewRuntimeConfig().
		WithMemoryLimitPages(r.Config.MemoryLimit)

	// Create runtime
	runtime := wazero.NewRuntimeWithConfig(ctx, config)

	// Compile module
	module, err := runtime.CompileModule(ctx, wasmBytes)
	if err != nil {
		runtime.Close(ctx)
		return NewWASMError(ErrCodeCompileFailed, "failed to compile module", map[string]interface{}{
			"error": err.Error(),
		})
	}

	// Validate module
	if err := r.validateModule(module); err != nil {
		runtime.Close(ctx)
		return err
	}

	// Store module and runtime
	r.Runtime = runtime
	r.Module = module

	return nil
}

// InstantiateModule instantiates the loaded module
func (r *Runtime) InstantiateModule(ctx context.Context) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.Module == nil {
		return NewWASMError(ErrCodeNoModuleLoaded, "no module loaded", nil)
	}

	// Register dummy import if needed
	imports := r.Module.ImportedFunctions()
	for _, imp := range imports {
		if imp.ModuleName() == "env" && imp.Name() == "dummy" {
			// Register a dummy function in the 'env' module
			_, err := r.Runtime.NewHostModuleBuilder("env").
				NewFunctionBuilder().
				WithFunc(func() {}).
				Export("dummy").
				Instantiate(ctx)
			if err != nil {
				return NewWASMError(ErrCodeInstantiateFailed, "failed to register dummy import", map[string]interface{}{
					"error": err.Error(),
				})
			}
			break
		}
	}

	// Instantiate WASI, which implements host functions needed for TinyGo to
	// implement `panic`.
	wasi_snapshot_preview1.MustInstantiate(ctx, r.Runtime)

	// Create instance
	instance, err := r.Runtime.InstantiateModule(ctx, r.Module, wazero.NewModuleConfig().WithName("").WithStartFunctions("_initialize"))
	if err != nil {
		return NewWASMError(ErrCodeInstantiateFailed, "failed to instantiate module", map[string]interface{}{
			"error": err.Error(),
		})
	}

	// Get memory
	memory := instance.Memory()
	if memory == nil {
		instance.Close(ctx)
		return NewWASMError(ErrCodeNoMemory, "module has no memory", nil)
	}

	// Store instance and memory
	r.instance = instance
	r.memory = memory

	return nil
}

// CallFunction calls a function in the WASM module
func (r *Runtime) CallFunction(ctx context.Context, name string, params ...interface{}) ([]uint64, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.instance == nil {
		return nil, NewWASMError(ErrCodeNoModuleLoaded, "no module loaded", nil)
	}

	// Create context with timeout if configured
	if r.Config.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, r.Config.Timeout)
		defer cancel()
	}

	// Get function
	fn := r.instance.ExportedFunction(name)
	if fn == nil {
		return nil, NewWASMError(ErrCodeFunctionNotFound, "function not found: "+name, nil)
	}

	// Marshal parameters
	wasmParams, err := r.marshalParams(params...)
	if err != nil {
		return nil, err
	}

	// Call function with panic recovery
	var results []uint64
	var callErr error
	func() {
		defer func() {
			if r := recover(); r != nil {
				callErr = NewWASMError(ErrCodePanic, "function call panicked", map[string]interface{}{
					"recover": r,
					"stack":   string(debug.Stack()),
				})
			}
		}()

		results, callErr = fn.Call(ctx, wasmParams...)
	}()

	if callErr != nil {
		return nil, NewWASMError(ErrCodeCallFailed, "function call failed", map[string]interface{}{
			"error": callErr.Error(),
		})
	}

	// Check for timeout
	if ctx.Err() == context.DeadlineExceeded {
		return nil, NewWASMError(ErrCodeTimeout, "function call timed out", nil)
	}

	return results, nil
}

// validateModule validates the module
func (r *Runtime) validateModule(module wazero.CompiledModule) error {
	// Check for required imports
	imports := module.ImportedFunctions()
	if len(imports) == 0 {
		return NewWASMError(ErrCodeNoImports, "module has no imported functions", nil)
	}

	// Check for required exports
	exports := module.ExportedFunctions()
	if len(exports) == 0 {
		return NewWASMError(ErrCodeNoExports, "module has no exported functions", nil)
	}

	return nil
}

// marshalParams converts Go values to WASM values
func (r *Runtime) marshalParams(params ...interface{}) ([]uint64, error) {
	if len(params) == 0 {
		return nil, nil
	}

	wasmParams := make([]uint64, 0, len(params))
	for _, param := range params {
		switch v := param.(type) {
		case int32:
			wasmParams = append(wasmParams, uint64(v))
		case int64:
			wasmParams = append(wasmParams, uint64(v))
		case uint32:
			wasmParams = append(wasmParams, uint64(v))
		case uint64:
			wasmParams = append(wasmParams, v)
		case float32:
			wasmParams = append(wasmParams, uint64(math.Float32bits(v)))
		case float64:
			wasmParams = append(wasmParams, math.Float64bits(v))
		case []byte:
			ptr, err := r.writeToMemory(v)
			if err != nil {
				return nil, err
			}
			wasmParams = append(wasmParams, uint64(ptr))
		case string:
			ptr, err := r.writeToMemory([]byte(v))
			if err != nil {
				return nil, err
			}
			wasmParams = append(wasmParams, uint64(ptr))
		default:
			return nil, NewWASMError(ErrCodeUnsupportedType, "unsupported parameter type", map[string]interface{}{
				"type": fmt.Sprintf("%T", param),
			})
		}
	}

	return wasmParams, nil
}

// unmarshalResults converts WASM values back to Go values
func (r *Runtime) unmarshalResults(results []uint64) ([]interface{}, error) {
	if len(results) == 0 {
		return nil, nil
	}

	// For now, just return the raw uint64 values
	// TODO: Implement proper type conversion based on function signature
	values := make([]interface{}, len(results))
	for i, v := range results {
		values[i] = v
	}

	return values, nil
}

// Close releases all resources
func (r *Runtime) Close(ctx context.Context) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	// Call cleanup functions
	for _, cleanup := range r.cleanupFuncs {
		if err := cleanup(); err != nil {
			return NewWASMError(ErrCodeCleanupFailed, "cleanup function failed", map[string]interface{}{
				"error": err.Error(),
			})
		}
	}

	// Close instance if exists
	if r.instance != nil {
		if err := r.instance.Close(ctx); err != nil {
			return NewWASMError(ErrCodeCloseFailed, "failed to close instance", map[string]interface{}{
				"error": err.Error(),
			})
		}
		r.instance = nil
	}

	// Close runtime if exists
	if r.Runtime != nil {
		if err := r.Runtime.Close(ctx); err != nil {
			return NewWASMError(ErrCodeCloseFailed, "failed to close runtime", map[string]interface{}{
				"error": err.Error(),
			})
		}
		r.Runtime = nil
	}

	return nil
}

// AddCleanup adds a cleanup function to be called when the runtime is closed
func (r *Runtime) AddCleanup(fn func() error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.cleanupFuncs = append(r.cleanupFuncs, fn)
}

// GetMemoryStats returns current memory statistics
func (r *Runtime) GetMemoryStats() (*MemoryStats, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.memory == nil {
		return nil, NewWASMError(ErrCodeNoMemory, "no memory available", nil)
	}

	return &MemoryStats{
		Usage:    r.memoryUsage.Load(),
		Allocs:   r.memoryAllocs.Load(),
		Frees:    r.memoryFrees.Load(),
		Size:     r.memory.Size(),
		Capacity: r.Config.MemoryLimit * 65536, // Convert pages to bytes
	}, nil
}

// writeToMemory writes data to WASM memory and returns the pointer
func (r *Runtime) writeToMemory(data []byte) (uint32, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.memory == nil {
		return 0, NewWASMError(ErrCodeNoMemory, "no memory available", nil)
	}

	// Get current memory size
	size := r.memory.Size()
	if size == 0 {
		return 0, NewWASMError(ErrCodeMemoryNotInit, "memory not initialized", nil)
	}

	// Grow memory if needed
	requiredSize := size + uint32(len(data))
	if requiredSize > r.Config.MemoryLimit*65536 { // Convert pages to bytes
		return 0, NewWASMError(ErrCodeMemoryExceeded, "memory limit exceeded", map[string]interface{}{
			"required_size": requiredSize,
			"memory_limit":  r.Config.MemoryLimit * 65536,
		})
	}
	r.memory.Grow(requiredSize/65536 + 1)

	// Write data to memory
	if !r.memory.Write(uint32(size), data) {
		return 0, NewWASMError(ErrCodeWriteFailed, "failed to write to memory", map[string]interface{}{
			"offset": size,
			"size":   len(data),
		})
	}

	// Update memory statistics
	r.memoryUsage.Add(uint64(len(data)))
	r.memoryAllocs.Add(1)

	return uint32(size), nil
}

// readFromMemory reads data from WASM memory
func (r *Runtime) readFromMemory(ptr uint32, size uint32) ([]byte, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.memory == nil {
		return nil, NewWASMError(ErrCodeNoMemory, "no memory available", nil)
	}

	// Check bounds
	if ptr+size > r.memory.Size() {
		return nil, NewWASMError(ErrCodeOutOfBounds, "memory access out of bounds", map[string]interface{}{
			"pointer": ptr,
			"size":    size,
			"limit":   r.memory.Size(),
		})
	}

	// Read data from memory
	data, ok := r.memory.Read(ptr, size)
	if !ok {
		return nil, NewWASMError(ErrCodeMemoryReadError, "failed to read from memory", map[string]interface{}{
			"pointer": ptr,
			"size":    size,
		})
	}

	return data, nil
}

// GetBuffer gets a buffer from the memory pool
func (r *Runtime) GetBuffer() *bytes.Buffer {
	if !r.Config.EnableMemoryPool {
		return bytes.NewBuffer(make([]byte, 0, r.Config.MemoryPoolInitialSize))
	}

	currBuff := r.MemoryPool.Get().([]byte)
	buf := bytes.NewBuffer(currBuff)

	return buf
}

// PutBuffer returns a buffer to the memory pool
func (r *Runtime) PutBuffer(buf *bytes.Buffer) error {
	if !r.Config.EnableMemoryPool || buf == nil {
		return nil
	}

	// Reset buffer
	buf.Reset()

	// Check if buffer is too large
	if buf.Cap() > r.Config.MemoryPoolMaxSize {
		return NewWASMError(ErrCodePoolPutFailed, "buffer too large for pool", map[string]interface{}{
			"capacity": buf.Cap(),
			"max_size": r.Config.MemoryPoolMaxSize,
		})
	}

	r.MemoryPool.Put(buf)
	return nil
}
