package wasm

import (
	"context"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
)

// MemoryRegion represents a contiguous region of memory
type MemoryRegion struct {
	Ptr         uint32    // Starting pointer
	Size        uint32    // Size in bytes
	Used        bool      // Whether the region is currently allocated
	Tag         string    // Optional tag for debugging
	AllocatedAt time.Time // When this region was allocated
}

// AdvancedMemoryPool extends the basic sync.Pool with size-based pools and metrics
type AdvancedMemoryPool struct {
	// Size-based pools for different buffer sizes
	pools map[int]*sync.Pool
	// Metrics for pool performance
	hits   atomic.Uint64
	misses atomic.Uint64
	reuses atomic.Uint64
	// Configuration
	maxPoolSize   int
	poolSizes     []int // Predefined pool sizes
	enableMetrics bool
	mu            sync.RWMutex
}

// MemoryTracker provides detailed memory allocation tracking
type MemoryTracker struct {
	// Region tracking
	regions     map[uint32]*MemoryRegion
	freeRegions []uint32 // Available region pointers

	// Allocation statistics
	totalAllocated    atomic.Uint64
	totalDeallocated  atomic.Uint64
	currentAllocated  atomic.Uint64
	peakAllocated     atomic.Uint64
	allocationCount   atomic.Uint64
	deallocationCount atomic.Uint64

	// Leak detection
	leakThreshold    time.Duration
	suspiciousAllocs map[uint32]time.Time

	// Thread safety
	mu sync.RWMutex

	// Configuration
	enableLeakDetection bool
	trackCallStacks     bool
	callStacks          map[uint32]string
}

// MemoryManager coordinates advanced memory management features
type MemoryManager struct {
	pool    *AdvancedMemoryPool
	tracker *MemoryTracker
	runtime *Runtime // Reference to parent runtime

	// Zero-copy features
	views map[uint32]*MemoryView // Active memory views

	// Configuration
	config *MemoryConfig
	mu     sync.RWMutex
}

// MemoryConfig holds configuration for advanced memory management
type MemoryConfig struct {
	// Pool configuration
	EnableAdvancedPool bool
	PoolSizes          []int
	MaxPoolSize        int

	// Tracking configuration
	EnableTracking      bool
	EnableLeakDetection bool
	LeakThreshold       time.Duration
	TrackCallStacks     bool

	// Zero-copy configuration
	EnableZeroCopy bool
	MaxViews       int

	// Debugging
	EnableDebugMode  bool
	DebugLogInterval time.Duration
}

// MemoryView represents a zero-copy view into memory
type MemoryView struct {
	Ptr        uint32
	Size       uint32
	RefCount   atomic.Int32
	ReadOnly   bool
	CreatedAt  time.Time
	LastAccess time.Time
}

// DefaultMemoryConfig returns a default memory configuration
func DefaultMemoryConfig() *MemoryConfig {
	return &MemoryConfig{
		EnableAdvancedPool:  true,
		PoolSizes:           []int{64, 256, 1024, 4096, 16384, 65536},
		MaxPoolSize:         1024 * 1024, // 1MB
		EnableTracking:      true,
		EnableLeakDetection: true,
		LeakThreshold:       time.Minute * 5, // 5 minutes
		TrackCallStacks:     false,           // Expensive, enable for debugging
		EnableZeroCopy:      true,
		MaxViews:            1000,
		EnableDebugMode:     false,
		DebugLogInterval:    time.Minute * 1,
	}
}

// NewAdvancedMemoryPool creates a new advanced memory pool
func NewAdvancedMemoryPool(config *MemoryConfig) *AdvancedMemoryPool {
	pool := &AdvancedMemoryPool{
		pools:         make(map[int]*sync.Pool),
		maxPoolSize:   config.MaxPoolSize,
		poolSizes:     config.PoolSizes,
		enableMetrics: true,
	}

	// Initialize size-based pools
	for _, size := range config.PoolSizes {
		size := size // Capture loop variable
		pool.pools[size] = &sync.Pool{
			New: func() interface{} {
				pool.misses.Add(1)
				return make([]byte, 0, size)
			},
		}
	}

	return pool
}

// GetBuffer returns a buffer from the appropriate pool
func (p *AdvancedMemoryPool) GetBuffer(size int) []byte {
	poolSize := p.findPoolSize(size)
	if poolSize == -1 {
		// Size too large for pooling
		return make([]byte, 0, size)
	}

	p.mu.RLock()
	pool, exists := p.pools[poolSize]
	p.mu.RUnlock()

	if !exists {
		return make([]byte, 0, size)
	}

	buffer := pool.Get().([]byte)
	p.hits.Add(1)

	// Reset buffer to requested size
	return buffer[:0]
}

// PutBuffer returns a buffer to the appropriate pool
func (p *AdvancedMemoryPool) PutBuffer(buffer []byte) {
	capacity := cap(buffer)
	poolSize := p.findPoolSize(capacity)

	if poolSize == -1 || capacity > p.maxPoolSize {
		// Don't pool oversized buffers
		return
	}

	p.mu.RLock()
	pool, exists := p.pools[poolSize]
	p.mu.RUnlock()

	if !exists {
		return
	}

	// Clear buffer data for security
	for i := range buffer {
		buffer[i] = 0
	}

	// Reset and return to pool
	buffer = buffer[:0]
	pool.Put(buffer)
	p.reuses.Add(1)
}

// findPoolSize finds the appropriate pool size for a given buffer size
func (p *AdvancedMemoryPool) findPoolSize(size int) int {
	for _, poolSize := range p.poolSizes {
		if size <= poolSize {
			return poolSize
		}
	}
	return -1 // Too large for any pool
}

// GetStats returns pool performance statistics
func (p *AdvancedMemoryPool) GetStats() map[string]uint64 {
	return map[string]uint64{
		"hits":   p.hits.Load(),
		"misses": p.misses.Load(),
		"reuses": p.reuses.Load(),
	}
}

// NewMemoryTracker creates a new memory tracker
func NewMemoryTracker(config *MemoryConfig) *MemoryTracker {
	tracker := &MemoryTracker{
		regions:             make(map[uint32]*MemoryRegion),
		freeRegions:         make([]uint32, 0),
		leakThreshold:       config.LeakThreshold,
		suspiciousAllocs:    make(map[uint32]time.Time),
		enableLeakDetection: config.EnableLeakDetection,
		trackCallStacks:     config.TrackCallStacks,
		callStacks:          make(map[uint32]string),
	}

	return tracker
}

// TrackAllocation records a memory allocation
func (t *MemoryTracker) TrackAllocation(ptr uint32, size uint32, tag string) {
	t.mu.Lock()
	defer t.mu.Unlock()

	region := &MemoryRegion{
		Ptr:         ptr,
		Size:        size,
		Used:        true,
		Tag:         tag,
		AllocatedAt: time.Now(),
	}

	t.regions[ptr] = region
	t.totalAllocated.Add(uint64(size))
	t.currentAllocated.Add(uint64(size))
	t.allocationCount.Add(1)

	// Update peak if necessary
	current := t.currentAllocated.Load()
	for {
		peak := t.peakAllocated.Load()
		if current <= peak || t.peakAllocated.CompareAndSwap(peak, current) {
			break
		}
	}

	// Track for leak detection
	if t.enableLeakDetection {
		t.suspiciousAllocs[ptr] = time.Now()
	}

	// Capture call stack if enabled
	if t.trackCallStacks {
		t.callStacks[ptr] = t.captureCallStack()
	}
}

// TrackDeallocation records a memory deallocation
func (t *MemoryTracker) TrackDeallocation(ptr uint32) {
	t.mu.Lock()
	defer t.mu.Unlock()

	region, exists := t.regions[ptr]
	if !exists {
		return // Double-free or invalid pointer
	}

	t.totalDeallocated.Add(uint64(region.Size))
	t.currentAllocated.Add(^uint64(region.Size - 1)) // Subtract
	t.deallocationCount.Add(1)

	delete(t.regions, ptr)
	t.freeRegions = append(t.freeRegions, ptr)

	// Remove from leak detection
	delete(t.suspiciousAllocs, ptr)
	delete(t.callStacks, ptr)
}

// CheckForLeaks identifies potential memory leaks
func (t *MemoryTracker) CheckForLeaks() []uint32 {
	if !t.enableLeakDetection {
		return nil
	}

	t.mu.RLock()
	defer t.mu.RUnlock()

	now := time.Now()
	var leaks []uint32

	for ptr, allocTime := range t.suspiciousAllocs {
		if now.Sub(allocTime) > t.leakThreshold {
			leaks = append(leaks, ptr)
		}
	}

	return leaks
}

// GetStats returns memory tracking statistics
func (t *MemoryTracker) GetStats() map[string]uint64 {
	t.mu.RLock()
	defer t.mu.RUnlock()

	return map[string]uint64{
		"total_allocated":    t.totalAllocated.Load(),
		"total_deallocated":  t.totalDeallocated.Load(),
		"current_allocated":  t.currentAllocated.Load(),
		"peak_allocated":     t.peakAllocated.Load(),
		"allocation_count":   t.allocationCount.Load(),
		"deallocation_count": t.deallocationCount.Load(),
		"active_regions":     uint64(len(t.regions)),
	}
}

// captureCallStack captures the current call stack for debugging
func (t *MemoryTracker) captureCallStack() string {
	const depth = 10
	var pcs [depth]uintptr
	n := runtime.Callers(3, pcs[:])
	frames := runtime.CallersFrames(pcs[:n])

	var stack string
	for {
		frame, more := frames.Next()
		stack += fmt.Sprintf("%s:%d\n", frame.Function, frame.Line)
		if !more {
			break
		}
	}

	return stack
}

// NewMemoryManager creates a new advanced memory manager
func NewMemoryManager(runtime *Runtime, config *MemoryConfig) *MemoryManager {
	if config == nil {
		config = DefaultMemoryConfig()
	}

	manager := &MemoryManager{
		runtime: runtime,
		config:  config,
		views:   make(map[uint32]*MemoryView),
	}

	if config.EnableAdvancedPool {
		manager.pool = NewAdvancedMemoryPool(config)
	}

	if config.EnableTracking {
		manager.tracker = NewMemoryTracker(config)
	}

	return manager
}

// AllocateBuffer allocates a buffer using the advanced pool
func (m *MemoryManager) AllocateBuffer(size int) []byte {
	if m.pool != nil {
		return m.pool.GetBuffer(size)
	}
	return make([]byte, 0, size)
}

// DeallocateBuffer returns a buffer to the pool
func (m *MemoryManager) DeallocateBuffer(buffer []byte) {
	if m.pool != nil {
		m.pool.PutBuffer(buffer)
	}
}

// AllocateMemory allocates memory in the WASM module with tracking
func (m *MemoryManager) AllocateMemory(ctx context.Context, size uint32, tag string) (uint32, error) {
	// Use runtime's WriteToMemory to allocate
	data := make([]byte, size)
	ptr, err := m.runtime.WriteToMemory(data)
	if err != nil {
		return 0, err
	}

	// Track the allocation
	if m.tracker != nil {
		m.tracker.TrackAllocation(ptr, size, tag)
	}

	return ptr, nil
}

// DeallocateMemory deallocates memory in the WASM module
func (m *MemoryManager) DeallocateMemory(ptr uint32) error {
	if m.tracker != nil {
		m.tracker.TrackDeallocation(ptr)
	}
	// Note: WASM linear memory doesn't support individual deallocation
	// This is primarily for tracking purposes
	return nil
}

// CreateView creates a zero-copy view into memory
func (m *MemoryManager) CreateView(ptr uint32, size uint32, readOnly bool) (*MemoryView, error) {
	if !m.config.EnableZeroCopy {
		return nil, fmt.Errorf("zero-copy views disabled")
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	if len(m.views) >= m.config.MaxViews {
		return nil, fmt.Errorf("maximum number of views reached")
	}

	view := &MemoryView{
		Ptr:        ptr,
		Size:       size,
		ReadOnly:   readOnly,
		CreatedAt:  time.Now(),
		LastAccess: time.Now(),
	}
	view.RefCount.Store(1)

	m.views[ptr] = view
	return view, nil
}

// ReleaseView releases a memory view
func (m *MemoryManager) ReleaseView(ptr uint32) {
	m.mu.Lock()
	defer m.mu.Unlock()

	view, exists := m.views[ptr]
	if !exists {
		return
	}

	if view.RefCount.Add(-1) <= 0 {
		delete(m.views, ptr)
	}
}

// GetMemoryInfo returns comprehensive memory information
func (m *MemoryManager) GetMemoryInfo() map[string]interface{} {
	info := make(map[string]interface{})

	if m.pool != nil {
		info["pool_stats"] = m.pool.GetStats()
	}

	if m.tracker != nil {
		info["tracking_stats"] = m.tracker.GetStats()
		info["potential_leaks"] = m.tracker.CheckForLeaks()
	}

	m.mu.RLock()
	info["active_views"] = len(m.views)
	m.mu.RUnlock()

	// Add runtime memory stats
	if stats, err := m.runtime.GetMemoryStats(); err == nil {
		info["runtime_stats"] = map[string]interface{}{
			"usage":    stats.Usage,
			"allocs":   stats.Allocs,
			"frees":    stats.Frees,
			"size":     stats.Size,
			"capacity": stats.Capacity,
		}
	}

	return info
}

// RunLeakDetection performs leak detection and returns suspicious allocations
func (m *MemoryManager) RunLeakDetection() []uint32 {
	if m.tracker != nil {
		return m.tracker.CheckForLeaks()
	}
	return nil
}

// Cleanup performs memory cleanup and releases resources
func (m *MemoryManager) Cleanup() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Release all views
	for ptr := range m.views {
		delete(m.views, ptr)
	}

	return nil
}

// AccessView safely accesses a memory view
func (v *MemoryView) Access() bool {
	if v.RefCount.Load() <= 0 {
		return false
	}
	v.LastAccess = time.Now()
	return true
}

// AddRef adds a reference to the view
func (v *MemoryView) AddRef() {
	v.RefCount.Add(1)
}

// Release releases a reference to the view
func (v *MemoryView) Release() bool {
	return v.RefCount.Add(-1) <= 0
}

// IsExpired checks if the view has been unused for too long
func (v *MemoryView) IsExpired(timeout time.Duration) bool {
	return time.Since(v.LastAccess) > timeout
}

// Size returns the size of the memory region in bytes
func (r *MemoryRegion) String() string {
	return fmt.Sprintf("Region{Ptr: 0x%x, Size: %d, Used: %t, Tag: %s, Age: %v}",
		r.Ptr, r.Size, r.Used, r.Tag, time.Since(r.AllocatedAt))
}
