package runtime

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
)

// Runtime represents a runtime instance for SpacetimeDB
type Runtime struct {
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
	// memory is the current module's memory
	memory []byte
}

// NewRuntime creates a new Runtime instance
func NewRuntime() *Runtime {
	return &Runtime{
		memory: make([]byte, 0),
	}
}

// AddCleanup adds a cleanup function to be called on Close
func (r *Runtime) AddCleanup(fn func() error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.cleanupFuncs = append(r.cleanupFuncs, fn)
}

// Close closes the runtime
func (r *Runtime) Close(ctx context.Context) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	var lastErr error
	for _, fn := range r.cleanupFuncs {
		if err := fn(); err != nil {
			lastErr = err
		}
	}
	return lastErr
}

// GetMemoryStats returns memory usage statistics
func (r *Runtime) GetMemoryStats() *MemoryStats {
	return &MemoryStats{
		Usage:  r.memoryUsage.Load(),
		Allocs: r.memoryAllocs.Load(),
		Frees:  r.memoryFrees.Load(),
	}
}

// MemoryStats holds memory usage statistics
type MemoryStats struct {
	Usage  uint64 // Current memory usage in bytes
	Allocs uint64 // Number of allocations
	Frees  uint64 // Number of deallocations
}

// CallFunction calls a function in the runtime
func (r *Runtime) CallFunction(ctx context.Context, name string, params ...interface{}) ([]uint64, error) {
	// TODO: Implement actual function call
	return []uint64{0}, nil
}

// ReadFromMemory reads data from memory
func (r *Runtime) ReadFromMemory(ptr uint32, size uint32) ([]byte, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if int(ptr)+int(size) > len(r.memory) {
		return nil, fmt.Errorf("memory read out of bounds: ptr=%d, size=%d, memory_size=%d", ptr, size, len(r.memory))
	}

	data := make([]byte, size)
	copy(data, r.memory[ptr:ptr+size])
	return data, nil
}

// WriteToMemory writes data to memory
func (r *Runtime) WriteToMemory(data []byte) (uint32, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	ptr := uint32(len(r.memory))
	r.memory = append(r.memory, data...)

	// Update memory stats
	r.memoryUsage.Add(uint64(len(data)))
	r.memoryAllocs.Add(1)

	return ptr, nil
}

// FreeMemory frees memory at the given pointer
func (r *Runtime) FreeMemory(ptr uint32, size uint32) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if int(ptr)+int(size) <= len(r.memory) {
		// Update memory stats
		r.memoryUsage.Add(^uint64(size - 1)) // Equivalent to subtracting size
		r.memoryFrees.Add(1)
	}
}
