package wasm

import (
	"bytes"
	"context"
	"fmt"
	"log"
	"math"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/db"
	rt "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/experimental"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
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
	ErrCodeUnreachable       = 10
	ErrCodeMemoryError       = 40
	ErrCodeInvalidWASM       = 50
	// Sink IDs
	ERROR_SINK_ID = 2 // Standard error sink ID
)

const (
	defaultErrorBufferSize = 1024
)

// ByteBufferRegistry manages byte source and sink handles
type ByteBufferRegistry struct {
	sources   map[uint32][]byte
	sinks     map[uint32][]byte
	errorSink []byte // Special sink for error messages
	nextID    uint32
	mu        sync.RWMutex
}

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
	MemoryPool  sync.Pool
	byteBuffers ByteBufferRegistry
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
	// db is the database instance
	db *db.Database
	// runtime is the base runtime instance
	baseRuntime *rt.Runtime
}

// simpleFunctionListener implements experimental.FunctionListener to log calls.
type simpleFunctionListener struct{}

// Before logs details before a function call.
func (s *simpleFunctionListener) Before(ctx context.Context, mod api.Module, def api.FunctionDefinition, params []uint64, stackIterator experimental.StackIterator) {
	log.Printf("[Wazero DEBUG] Before call: Module: %s, Function: %s, Index: %d, Params: %v", mod.Name(), def.Name(), def.Index(), params)
	// No context return
}

// After logs details after a function call.
func (s *simpleFunctionListener) After(ctx context.Context, mod api.Module, def api.FunctionDefinition, results []uint64) {
	log.Printf("[Wazero DEBUG] After call (Success): Module: %s, Function: %s, Index: %d, Results: %v", mod.Name(), def.Name(), def.Index(), results)
}

// Abort logs details if a function call is aborted.
func (s *simpleFunctionListener) Abort(ctx context.Context, mod api.Module, def api.FunctionDefinition, err error) {
	log.Printf("[Wazero DEBUG] Abort call: Module: %s, Function: %s, Error: %v", mod.Name(), def.Name(), err)
}

// simpleFunctionListenerFactory implements experimental.FunctionListenerFactory.
type simpleFunctionListenerFactory struct{}

// NewFunctionListener returns a new simpleFunctionListener.
func (f *simpleFunctionListenerFactory) NewFunctionListener(def api.FunctionDefinition) experimental.FunctionListener {
	log.Printf("[Wazero DEBUG] Factory creating new listener for function: %s (Index: %d)", def.Name(), def.Index())
	return &simpleFunctionListener{}
}

// NewRuntime creates a new Runtime instance with the given configuration
func NewRuntime(config *Config) (*Runtime, error) {
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

	// Create base runtime instance
	r.baseRuntime = rt.New()

	// Create database instance
	dbInst, errDB := db.NewDatabase(r.baseRuntime)
	if errDB != nil {
		return nil, errDB
	}
	r.db = dbInst

	// Create a wazero runtime configuration.
	ctx := context.Background()
	wazeroConfig := wazero.NewRuntimeConfig().
		WithMemoryLimitPages(r.Config.MemoryLimit)

	// Enable FunctionListenerFactory using experimental.WithFunctionListenerFactory
	// This uses the definition from wazero@v1.8.0/experimental/listener.go
	// which relies on an internal key from "github.com/tetratelabs/wazero/internal/expctxkeys"
	ctx = experimental.WithFunctionListenerFactory(ctx, new(simpleFunctionListenerFactory))

	wzRuntime := wazero.NewRuntimeWithConfig(ctx, wazeroConfig)
	r.Runtime = wzRuntime
	r.byteBuffers = ByteBufferRegistry{
		sources: make(map[uint32][]byte),
		sinks:   make(map[uint32][]byte),
		nextID:  1, // Start from 1, 0 could be an invalid handle
	}

	return r, nil
}

// LoadModule loads and compiles a WASM module
func (r *Runtime) LoadModule(ctx context.Context, wasmBytes []byte) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	// Ensure r.Runtime (the wazero.Runtime) is initialized.
	// This should have been done in NewRuntime.
	if r.Runtime == nil {
		return NewWASMError(ErrCodeRuntimeInit, "wazero runtime not initialized in LoadModule", nil)
	}

	// Compile module using the existing r.Runtime
	module, err := r.Runtime.CompileModule(ctx, wasmBytes)
	if err != nil {
		return NewWASMError(ErrCodeCompileFailed, "failed to compile module", map[string]interface{}{
			"error": err,
		})
	}

	// Validate module
	if err := r.validateModule(module); err != nil {
		return err
	}

	r.Module = module
	return nil
}

// InstantiateModule instantiates a WASM module
func (r *Runtime) InstantiateModule(ctx context.Context, moduleName string, withWASI bool) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.Module == nil {
		return NewWASMError(ErrCodeNoModuleLoaded, "no module loaded", nil)
	}

	// Create a new context with timeout
	timeoutCtx, cancel := context.WithTimeout(ctx, r.Config.Timeout)
	defer cancel()

	if withWASI {
		fmt.Println("[DEBUG] Attempting to instantiate WASI...")
		// Instantiate WASI
		_, wasiErr := wasi_snapshot_preview1.Instantiate(timeoutCtx, r.Runtime)
		if wasiErr != nil {
			fmt.Printf("[DEBUG] Failed to instantiate WASI: %v\n", wasiErr)
			return NewWASMError(ErrCodeInstantiateFailed, "failed to instantiate WASI", map[string]interface{}{
				"error": wasiErr.Error(),
			})
		}
		fmt.Println("[DEBUG] WASI instantiated.")
	}

	// Instantiate the spacetime host module
	fmt.Println("[DEBUG] Attempting to instantiate spacetimeModule from InstantiateModule...")
	spacetimeModuleBuilder := NewSpacetimeModule(r)
	spacetimeErr := spacetimeModuleBuilder.Instantiate(timeoutCtx, r.Runtime)
	if spacetimeErr != nil {
		fmt.Printf("[DEBUG] Failed to instantiate spacetime module: %v\n", spacetimeErr)
		return NewWASMError(ErrCodeInstantiateFailed, "failed to instantiate spacetime module", map[string]interface{}{
			"error": spacetimeErr.Error(),
		})
	}
	fmt.Println("[DEBUG] spacetimeModule instantiated successfully from InstantiateModule.")

	// Print module info for debugging
	fmt.Printf("[DEBUG] Instantiating module %s with the following imports:\n", moduleName)
	for _, imp := range r.Module.ImportedFunctions() {
		moduleName, name, _ := imp.Import()
		fmt.Printf("[DEBUG]   Import: %s.%s\n", moduleName, name)
	}

	// Instantiate the module
	fmt.Printf("[DEBUG] Calling InstantiateModule for %s\n", moduleName)
	instance, err := r.Runtime.InstantiateModule(timeoutCtx, r.Module, wazero.NewModuleConfig().WithName(moduleName))
	if err != nil {
		fmt.Printf("[DEBUG] Failed to instantiate module: %v\n", err)
		return NewWASMError(ErrCodeInstantiateFailed, "failed to instantiate module", map[string]interface{}{
			"error": err.Error(),
		})
	}
	fmt.Println("[DEBUG] Module instantiated successfully")

	// Get memory
	memory := instance.Memory()
	if memory == nil {
		fmt.Println("[DEBUG] Module does not export memory")
		return NewWASMError(ErrCodeMemoryNotInit, "module does not export memory", nil)
	}

	// Update instance and memory
	r.instance = instance
	r.memory = memory

	return nil
}

// validateModule validates a WASM module
func (r *Runtime) validateModule(module wazero.CompiledModule) error {
	// Allow modules with no exports (used in certain unit tests)
	return nil
}

// CallFunction calls a function in the runtime
func (r *Runtime) CallFunction(ctx context.Context, name string, params ...interface{}) ([]uint64, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	// Check if module is instantiated
	if r.instance == nil {
		return nil, NewWASMError(ErrCodeNoModuleLoaded, "no module loaded", nil)
	}

	// Get function
	fn := r.instance.ExportedFunction(name)
	if fn == nil {
		return nil, NewWASMError(ErrCodeFunctionNotFound, "function not found", map[string]interface{}{
			"name": name,
		})
	}

	// Marshal parameters
	args, err := r.marshalParams(params...)
	if err != nil {
		return nil, err
	}

	// Call function
	results, err := fn.Call(ctx, args...)
	if err != nil {
		return nil, NewWASMError(ErrCodeCallFailed, "failed to call function", map[string]interface{}{
			"error": err.Error(),
		})
	}

	return results, nil
}

// marshalParams marshals parameters for a function call
func (r *Runtime) marshalParams(params ...interface{}) ([]uint64, error) {
	args := make([]uint64, len(params))
	for i, param := range params {
		switch v := param.(type) {
		case uint32:
			args[i] = uint64(v)
		case uint64:
			args[i] = v
		case int32:
			args[i] = uint64(v)
		case int64:
			args[i] = uint64(v)
		case float32:
			args[i] = uint64(math.Float32bits(v))
		case float64:
			args[i] = math.Float64bits(v)
		default:
			return nil, NewWASMError(ErrCodeUnsupportedType, "unsupported parameter type", map[string]interface{}{
				"type": fmt.Sprintf("%T", v),
			})
		}
	}
	return args, nil
}

// unmarshalResults unmarshals results from a function call
func (r *Runtime) unmarshalResults(results []uint64) ([]interface{}, error) {
	values := make([]interface{}, len(results))
	for i, result := range results {
		values[i] = result
	}
	return values, nil
}

// RegisterByteSource registers a byte slice as a source
func (r *Runtime) RegisterByteSource(data []byte) uint32 {
	r.byteBuffers.mu.Lock()
	defer r.byteBuffers.mu.Unlock()

	id := r.byteBuffers.nextID
	r.byteBuffers.nextID++
	r.byteBuffers.sources[id] = data
	return id
}

// GetByteSource gets a byte source by ID
func (r *Runtime) GetByteSource(id uint32) ([]byte, bool) {
	r.byteBuffers.mu.RLock()
	defer r.byteBuffers.mu.RUnlock()

	fmt.Printf("[DEBUG] GetByteSource: Requested source ID %d\n", id)

	if id == 0 {
		fmt.Printf("[DEBUG] GetByteSource: ID 0 is invalid\n")
		return nil, false
	}

	source, ok := r.byteBuffers.sources[id]
	if !ok {
		fmt.Printf("[DEBUG] GetByteSource: Source ID %d not found in registry (available IDs: %v)\n",
			id, keysOfMap(r.byteBuffers.sources))
		return nil, false
	}

	fmt.Printf("[DEBUG] GetByteSource: Found source ID %d with %d bytes\n", id, len(source))
	return source, true
}

// Helper function to get keys of a map
func keysOfMap(m map[uint32][]byte) []uint32 {
	keys := make([]uint32, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	return keys
}

// UnregisterByteSource unregisters a byte source
func (r *Runtime) UnregisterByteSource(handle uint32) {
	r.byteBuffers.mu.Lock()
	defer r.byteBuffers.mu.Unlock()

	delete(r.byteBuffers.sources, handle)
}

// RegisterByteSink registers a byte sink
func (r *Runtime) RegisterByteSink(capacity uint32) uint32 {
	r.byteBuffers.mu.Lock()
	defer r.byteBuffers.mu.Unlock()

	id := r.byteBuffers.nextID
	r.byteBuffers.nextID++
	r.byteBuffers.sinks[id] = make([]byte, 0, capacity)
	return id
}

// WriteByteSink writes data to a byte sink
func (r *Runtime) WriteByteSink(id uint32, data []byte) bool {
	r.byteBuffers.mu.Lock()
	defer r.byteBuffers.mu.Unlock()

	fmt.Printf("[DEBUG] WriteByteSink: Writing %d bytes to sink ID %d\n", len(data), id)

	// If this is error sink, store it for retrieval later
	if id == ERROR_SINK_ID {
		r.byteBuffers.errorSink = append(r.byteBuffers.errorSink, data...)
		fmt.Printf("[DEBUG] WriteByteSink: Added %d bytes to error sink, total now %d bytes\n",
			len(data), len(r.byteBuffers.errorSink))

		// Print error message if short
		if len(data) < 100 {
			fmt.Printf("[DEBUG] WriteByteSink: Error message: %s\n", string(data))
		}
		return true
	}

	// For regular sinks, just store the data
	sink, ok := r.byteBuffers.sinks[id]
	if !ok {
		fmt.Printf("[DEBUG] WriteByteSink: Creating new sink with ID %d\n", id)
		r.byteBuffers.sinks[id] = data
		return true
	}

	// Append to existing sink
	r.byteBuffers.sinks[id] = append(sink, data...)
	fmt.Printf("[DEBUG] WriteByteSink: Appended to sink ID %d, now %d bytes\n",
		id, len(r.byteBuffers.sinks[id]))
	return true
}

// ReadByteSink reads data from a byte sink
func (r *Runtime) ReadByteSink(handle uint32) ([]byte, bool) {
	r.byteBuffers.mu.RLock()
	defer r.byteBuffers.mu.RUnlock()

	data, ok := r.byteBuffers.sinks[handle]
	return data, ok
}

// UnregisterByteSink unregisters a byte sink
func (r *Runtime) UnregisterByteSink(handle uint32) {
	r.byteBuffers.mu.Lock()
	defer r.byteBuffers.mu.Unlock()

	delete(r.byteBuffers.sinks, handle)
}

// Close closes the runtime
func (r *Runtime) Close(ctx context.Context) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	// Close runtime
	if r.Runtime != nil {
		if err := r.Runtime.Close(ctx); err != nil {
			return NewWASMError(ErrCodeCloseFailed, "failed to close runtime", map[string]interface{}{
				"error": err.Error(),
			})
		}
	}

	// Call cleanup functions
	var lastErr error
	for _, fn := range r.cleanupFuncs {
		if err := fn(); err != nil {
			lastErr = err
		}
	}

	return lastErr
}

// AddCleanup adds a cleanup function to be called on Close
func (r *Runtime) AddCleanup(fn func() error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.cleanupFuncs = append(r.cleanupFuncs, fn)
}

// GetMemoryStats returns memory usage statistics
func (r *Runtime) GetMemoryStats() (*MemoryStats, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.memory == nil {
		return nil, NewWASMError(ErrCodeMemoryNotInit, "memory not initialized", nil)
	}

	return &MemoryStats{
		Usage:    r.memoryUsage.Load(),
		Allocs:   r.memoryAllocs.Load(),
		Frees:    r.memoryFrees.Load(),
		Size:     r.memory.Size(),
		Capacity: r.Config.MemoryLimit * 65536, // Convert pages to bytes
	}, nil
}

// WriteToMemory writes data to memory
func (r *Runtime) WriteToMemory(data []byte) (uint32, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.memory == nil {
		return 0, NewWASMError(ErrCodeMemoryNotInit, "memory not initialized", nil)
	}

	ptrToWriteAt := r.memory.Size() // Current end of memory is where we'll try to write
	dataLen := uint32(len(data))

	if dataLen == 0 {
		return ptrToWriteAt, nil // Nothing to write, return current end as a valid pointer (conventionally)
	}

	neededTotalSize := ptrToWriteAt + dataLen

	// Check if the allocation would exceed the configured memory limit
	wasmPageSize := uint32(65536)
	if neededTotalSize > r.Config.MemoryLimit*wasmPageSize {
		return 0, NewWASMError(ErrCodeMemoryExceeded, fmt.Sprintf("memory limit exceeded: needed %d, limit %d", neededTotalSize, r.Config.MemoryLimit*wasmPageSize), nil)
	}

	// If current memory is not large enough, try to grow it
	if neededTotalSize > r.memory.Size() {
		currentSizeBytes := r.memory.Size()
		neededBytes := neededTotalSize - currentSizeBytes             // Additional bytes needed beyond current size
		deltaPages := (neededBytes + wasmPageSize - 1) / wasmPageSize // Number of pages to grow by

		if deltaPages > 0 { // Only grow if actually needed
			if _, ok := r.memory.Grow(deltaPages); !ok {
				return 0, NewWASMError(ErrCodeMemoryExceeded, fmt.Sprintf("failed to grow memory by %d pages", deltaPages), nil)
			}
		}
		// After successful grow, r.memory.Size() is updated. ptrToWriteAt remains the original end.
	}

	// Write data to memory at the original end (which is now hopefully valid space)
	if !r.memory.Write(ptrToWriteAt, data) {
		return 0, NewWASMError(ErrCodeWriteFailed, fmt.Sprintf("failed to write %d bytes at offset %d (memory size: %d)", dataLen, ptrToWriteAt, r.memory.Size()), nil)
	}

	// Update memory usage stats
	r.memoryUsage.Add(uint64(dataLen))
	r.memoryAllocs.Add(1)

	return ptrToWriteAt, nil
}

// ReadFromMemory reads data from memory
func (r *Runtime) ReadFromMemory(ptr uint32, size uint32) ([]byte, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.memory == nil {
		return nil, NewWASMError(ErrCodeMemoryNotInit, "memory not initialized", nil)
	}

	// Check bounds
	if ptr+size > r.memory.Size() {
		return nil, NewWASMError(ErrCodeOutOfBounds, "memory access out of bounds", nil)
	}

	// Read data from memory
	data, ok := r.memory.Read(ptr, size)
	if !ok {
		return nil, NewWASMError(ErrCodeMemoryReadError, "failed to read from memory", nil)
	}

	return data, nil
}

// GetBuffer gets a buffer from the pool
func (r *Runtime) GetBuffer() *bytes.Buffer {
	if !r.Config.EnableMemoryPool {
		return &bytes.Buffer{}
	}
	// Get a slice from pool (may contain leftover bytes). Return a zero-length
	// view so callers always see an empty buffer.
	slice := r.MemoryPool.Get().([]byte)
	return bytes.NewBuffer(slice[:0])
}

// PutBuffer puts a buffer back in the pool
func (r *Runtime) PutBuffer(buf *bytes.Buffer) error {
	if !r.Config.EnableMemoryPool {
		return nil
	}

	data := buf.Bytes()

	// Ignore over-sized buffers – let GC reclaim them.
	if len(data) > r.Config.MemoryPoolMaxSize {
		return nil
	}

	// Zero the slice to avoid leaking data across calls then reset and return
	for i := range data {
		data[i] = 0
	}
	buf.Reset()
	r.MemoryPool.Put(data)
	return nil
}

// UnmarshalResults unmarshals results from a function call
func (r *Runtime) UnmarshalResults(results []uint64) ([]interface{}, error) {
	return r.unmarshalResults(results)
}

// BaseRuntime returns the base runtime instance
func (r *Runtime) BaseRuntime() *rt.Runtime {
	return r.baseRuntime
}

// WriteToMemoryAt writes data to memory at a specific pointer
func (r *Runtime) WriteToMemoryAt(ptr uint32, data []byte) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.memory == nil {
		return NewWASMError(ErrCodeMemoryNotInit, "memory not initialized", nil)
	}

	// Write data to memory
	if !r.memory.Write(ptr, data) {
		return NewWASMError(ErrCodeWriteFailed, "failed to write to memory", nil)
	}

	return nil
}

// CallReducer invokes the __call_reducer__ FFI function in the WASM module.
func (r *Runtime) CallReducer(
	ctx context.Context,
	reducerId uint32,
	senderIdentity [4]uint64,
	connectionId [2]uint64,
	timestamp uint64, // microseconds
	reducerArgs []byte,
) (string, error) {
	r.mu.RLock()
	instance := r.instance
	r.mu.RUnlock()

	if instance == nil {
		return "", NewWASMError(ErrCodeNoModuleLoaded, "no module instance available", nil)
	}

	fn := instance.ExportedFunction("__call_reducer__")
	if fn == nil {
		return "", NewWASMError(ErrCodeFunctionNotFound, "function __call_reducer__ not found", nil)
	}

	// Register reducer arguments as a byte source
	fmt.Printf("[DEBUG] Creating byte source for reducer args: %s\n", string(reducerArgs))
	argsSource := r.RegisterByteSource(reducerArgs)
	defer r.UnregisterByteSource(argsSource)

	// Register error buffer as a byte sink
	errorSinkCapacity := uint32(defaultErrorBufferSize)
	fmt.Printf("[DEBUG] Creating error sink with capacity: %d bytes\n", errorSinkCapacity)
	errorSink := r.RegisterByteSink(errorSinkCapacity)
	defer r.UnregisterByteSink(errorSink)

	// Assemble parameters for __call_reducer__
	wazeroParams := make([]uint64, 10)
	wazeroParams[0] = uint64(reducerId)
	wazeroParams[1] = senderIdentity[0]
	wazeroParams[2] = senderIdentity[1]
	wazeroParams[3] = senderIdentity[2]
	wazeroParams[4] = senderIdentity[3]
	wazeroParams[5] = connectionId[0]
	wazeroParams[6] = connectionId[1]
	wazeroParams[7] = timestamp
	wazeroParams[8] = uint64(argsSource) // BytesSource handle
	wazeroParams[9] = uint64(errorSink)  // BytesSink handle

	// Print debug information
	fmt.Printf("[DEBUG] Calling __call_reducer__ with params:\n")
	fmt.Printf("[DEBUG]   ReducerId: %d\n", reducerId)
	fmt.Printf("[DEBUG]   SenderIdentity: %v\n", senderIdentity)
	fmt.Printf("[DEBUG]   ConnectionId: %v\n", connectionId)
	fmt.Printf("[DEBUG]   Timestamp: %d\n", timestamp)
	fmt.Printf("[DEBUG]   ArgsSource: %d (size: %d bytes)\n", argsSource, len(reducerArgs))
	fmt.Printf("[DEBUG]   Args: %s\n", string(reducerArgs))
	fmt.Printf("[DEBUG]   ErrorSink: %d\n", errorSink)

	// Try to inspect the WASM module
	dumpByteSourceForDebugging(r, argsSource)

	// Call with timeout context
	timeoutCtx, cancel := context.WithTimeout(ctx, r.Config.Timeout)
	defer cancel()

	results, callErr := fn.Call(timeoutCtx, wazeroParams...)
	if callErr != nil {
		// Add debug info for errors
		fmt.Printf("[DEBUG] __call_reducer__ failed with error: %v\n", callErr)

		if timeoutCtx.Err() == context.DeadlineExceeded {
			return "", NewWASMError(ErrCodeTimeout, "reducer call timed out", map[string]interface{}{"error": callErr})
		} else if timeoutCtx.Err() == context.Canceled {
			return "", NewWASMError(ErrCodeContextCanceled, "reducer call context canceled", map[string]interface{}{"error": callErr})
		}
		// Fallback for other call errors
		return "", NewWASMError(ErrCodeCallFailed, "failed to call __call_reducer__", map[string]interface{}{"error": callErr})
	}

	// Process results
	fmt.Printf("[DEBUG] __call_reducer__ returned results: %v\n", results)

	if len(results) < 1 {
		return "", NewWASMError(ErrCodeCallFailed, "__call_reducer__ did not return a status code", nil)
	}

	statusCode := int16(results[0]) // i16 status code
	fmt.Printf("[DEBUG] __call_reducer__ status code: %d\n", statusCode)

	if statusCode != 0 {
		// Reducer signalled an error. Read the error message from the error buffer.
		errorData, ok := r.ReadByteSink(errorSink)
		if !ok {
			// Failed to read the error message, but we know there was a reducer error.
			return fmt.Sprintf("Reducer error (status %d), but failed to read error message", statusCode), nil
		}

		// Find the first null terminator for the actual message length
		actualLen := len(errorData)
		for i, b := range errorData {
			if b == 0 {
				actualLen = i
				break
			}
		}

		errorMsg := string(errorData[:actualLen])
		fmt.Printf("[DEBUG] Reducer error message: %s\n", errorMsg)
		return errorMsg, nil
	}

	return "", nil // Success
}

// Helper function for debugging byte sources
func dumpByteSourceForDebugging(r *Runtime, sourceID uint32) {
	sourceData, ok := r.GetByteSource(sourceID)
	if !ok {
		fmt.Printf("[DEBUG DUMP] Source %d not found\n", sourceID)
		return
	}

	fmt.Printf("[DEBUG DUMP] Source %d contents (%d bytes):\n", sourceID, len(sourceData))
	// Dump as hex and as string
	fmt.Printf("  HEX: ")
	for i, b := range sourceData {
		if i > 32 { // Limit output
			fmt.Printf("...")
			break
		}
		fmt.Printf("%02x ", b)
	}
	fmt.Printf("\n")

	// Dump as ASCII/String
	fmt.Printf("  STR: %s\n", string(sourceData))
}

// ListExports returns a list of exported function names from the instantiated module
func (r *Runtime) ListExports() ([]string, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if r.instance == nil {
		return nil, NewWASMError(ErrCodeNoModuleLoaded, "no module instance loaded", nil)
	}

	var exports []string

	// Get exported functions using ExportedFunctionDefinitions
	for name := range r.instance.ExportedFunctionDefinitions() {
		exports = append(exports, name)
	}

	return exports, nil
}

// newByteBufferRegistry creates a new byte buffer registry.
func newByteBufferRegistry() *ByteBufferRegistry {
	return &ByteBufferRegistry{
		sources:   make(map[uint32][]byte),
		sinks:     make(map[uint32][]byte),
		errorSink: make([]byte, 0, 1024), // Initialize with 1KB capacity
		nextID:    1,
	}
}
