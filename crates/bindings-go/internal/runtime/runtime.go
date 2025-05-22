package runtime

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
)

// Runtime represents a lightweight runtime wrapper used by the in-memory DB
// and by the wasm host shim.  It intentionally has **no dependency** on
// internal/wasm or internal/db so that we avoid import cycles
// (db -> runtime, wasm -> db).  The wasm host embeds *runtime.Runtime, while
// other packages just need the memory helpers and cleanup logic.

type Runtime struct {
	mu sync.RWMutex // protects memory and cleanup lists

	memory []byte

	memoryUsage  atomic.Uint64
	memoryAllocs atomic.Uint64
	memoryFrees  atomic.Uint64

	cleanup []func() error
}

func New() *Runtime { return &Runtime{memory: make([]byte, 0)} }

// AddCleanup registers a func that will run when Close() is invoked.
func (r *Runtime) AddCleanup(f func() error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.cleanup = append(r.cleanup, f)
}

// Close executes all registered cleanup funcs.
func (r *Runtime) Close(ctx context.Context) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	var last error
	for _, f := range r.cleanup {
		if err := f(); err != nil {
			last = err
		}
	}
	return last
}

// Memory helpers -----------------------------------------------------------

type MemStats struct {
	Usage  uint64
	Allocs uint64
	Frees  uint64
}

func (r *Runtime) Stats() MemStats {
	return MemStats{
		Usage:  r.memoryUsage.Load(),
		Allocs: r.memoryAllocs.Load(),
		Frees:  r.memoryFrees.Load(),
	}
}

func (r *Runtime) Read(ptr, size uint32) ([]byte, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	if int(ptr)+int(size) > len(r.memory) {
		return nil, fmt.Errorf("memory read oob: ptr=%d size=%d mem=%d", ptr, size, len(r.memory))
	}
	out := make([]byte, size)
	copy(out, r.memory[ptr:ptr+size])
	return out, nil
}

func (r *Runtime) Write(data []byte) uint32 {
	r.mu.Lock()
	defer r.mu.Unlock()
	ptr := uint32(len(r.memory))
	r.memory = append(r.memory, data...)
	r.memoryUsage.Add(uint64(len(data)))
	r.memoryAllocs.Add(1)
	return ptr
}

func (r *Runtime) Free(ptr, size uint32) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if int(ptr)+int(size) <= len(r.memory) {
		r.memoryUsage.Add(^uint64(size - 1))
		r.memoryFrees.Add(1)
	}
}

// Backward-compat helper for packages that still expect internal/runtime.NewRuntime().
// Prefer using runtime.New going forward.
func NewRuntime() *Runtime { return New() }
