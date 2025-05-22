package wasm

import (
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDefaultMemoryConfig(t *testing.T) {
	config := DefaultMemoryConfig()

	assert.True(t, config.EnableAdvancedPool)
	assert.True(t, config.EnableTracking)
	assert.True(t, config.EnableLeakDetection)
	assert.Equal(t, time.Minute*5, config.LeakThreshold)
	assert.True(t, config.EnableZeroCopy)
	assert.Equal(t, 1000, config.MaxViews)
	assert.Equal(t, []int{64, 256, 1024, 4096, 16384, 65536}, config.PoolSizes)
}

func TestNewAdvancedMemoryPool(t *testing.T) {
	config := DefaultMemoryConfig()
	pool := NewAdvancedMemoryPool(config)

	assert.NotNil(t, pool)
	assert.Equal(t, config.MaxPoolSize, pool.maxPoolSize)
	assert.Equal(t, config.PoolSizes, pool.poolSizes)
	assert.True(t, pool.enableMetrics)
	assert.Len(t, pool.pools, len(config.PoolSizes))
}

func TestAdvancedMemoryPool_GetPutBuffer(t *testing.T) {
	config := DefaultMemoryConfig()
	config.MaxPoolSize = 32
	pool := NewAdvancedMemoryPool(config)

	tests := []struct {
		name string
		size int
	}{
		{"small buffer", 2},
		{"medium buffer", 16},
		{"oversized buffer", 33}, // Larger than max pool size
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Get buffer
			buffer := pool.GetBuffer(tt.size)
			assert.NotNil(t, buffer)
			assert.Equal(t, 0, len(buffer))
			assert.GreaterOrEqual(t, cap(buffer), tt.size)

			// Use buffer
			for i := 0; i < tt.size && i < cap(buffer); i++ {
				buffer = append(buffer, byte(i%256))
			}

			// Store buffer content before putting back
			originalContent := make([]byte, len(buffer))
			copy(originalContent, buffer)

			// Put buffer back
			pool.PutBuffer(buffer)

			// Verify buffer was actually used (had non-zero content)
			hasNonZero := false
			for _, b := range originalContent {
				if b != 0 {
					hasNonZero = true
					break
				}
			}
			assert.True(t, hasNonZero, "buffer should have had non-zero content before clearing")
		})
	}
}

func TestAdvancedMemoryPool_Stats(t *testing.T) {
	config := DefaultMemoryConfig()
	pool := NewAdvancedMemoryPool(config)

	// Initially, stats should be zero
	stats := pool.GetStats()
	assert.Equal(t, uint64(0), stats["hits"])
	assert.Equal(t, uint64(0), stats["misses"])
	assert.Equal(t, uint64(0), stats["reuses"])

	// Get and put some buffers
	buffer1 := pool.GetBuffer(64)
	buffer2 := pool.GetBuffer(256)
	pool.PutBuffer(buffer1)
	pool.PutBuffer(buffer2)

	// Get same size buffers again (should be hits)
	buffer3 := pool.GetBuffer(64)
	buffer4 := pool.GetBuffer(256)
	pool.PutBuffer(buffer3)
	pool.PutBuffer(buffer4)

	stats = pool.GetStats()
	assert.Greater(t, stats["hits"], uint64(0))
	assert.Greater(t, stats["reuses"], uint64(0))
}

func TestNewMemoryTracker(t *testing.T) {
	config := DefaultMemoryConfig()
	tracker := NewMemoryTracker(config)

	assert.NotNil(t, tracker)
	assert.Equal(t, config.LeakThreshold, tracker.leakThreshold)
	assert.Equal(t, config.EnableLeakDetection, tracker.enableLeakDetection)
	assert.Equal(t, config.TrackCallStacks, tracker.trackCallStacks)
	assert.NotNil(t, tracker.regions)
	assert.NotNil(t, tracker.suspiciousAllocs)
}

func TestMemoryTracker_TrackAllocation(t *testing.T) {
	config := DefaultMemoryConfig()
	tracker := NewMemoryTracker(config)

	ptr := uint32(0x1000)
	size := uint32(256)
	tag := "test_allocation"

	tracker.TrackAllocation(ptr, size, tag)

	// Verify allocation was tracked
	stats := tracker.GetStats()
	assert.Equal(t, uint64(size), stats["total_allocated"])
	assert.Equal(t, uint64(size), stats["current_allocated"])
	assert.Equal(t, uint64(1), stats["allocation_count"])
	assert.Equal(t, uint64(1), stats["active_regions"])

	// Check the region was created
	tracker.mu.RLock()
	region, exists := tracker.regions[ptr]
	tracker.mu.RUnlock()

	assert.True(t, exists)
	assert.Equal(t, ptr, region.Ptr)
	assert.Equal(t, size, region.Size)
	assert.Equal(t, tag, region.Tag)
	assert.True(t, region.Used)
}

func TestMemoryTracker_TrackDeallocation(t *testing.T) {
	config := DefaultMemoryConfig()
	tracker := NewMemoryTracker(config)

	ptr := uint32(0x1000)
	size := uint32(256)
	tag := "test_allocation"

	// First allocate
	tracker.TrackAllocation(ptr, size, tag)

	// Then deallocate
	tracker.TrackDeallocation(ptr)

	// Verify deallocation was tracked
	stats := tracker.GetStats()
	assert.Equal(t, uint64(size), stats["total_allocated"])
	assert.Equal(t, uint64(size), stats["total_deallocated"])
	assert.Equal(t, uint64(0), stats["current_allocated"])
	assert.Equal(t, uint64(1), stats["deallocation_count"])
	assert.Equal(t, uint64(0), stats["active_regions"])
}

func TestMemoryTracker_LeakDetection(t *testing.T) {
	config := DefaultMemoryConfig()
	config.LeakThreshold = time.Millisecond * 10 // Very short threshold for testing
	tracker := NewMemoryTracker(config)

	ptr := uint32(0x1000)
	size := uint32(256)
	tag := "potentially_leaked"

	tracker.TrackAllocation(ptr, size, tag)

	// Initially no leaks
	leaks := tracker.CheckForLeaks()
	assert.Len(t, leaks, 0)

	// Wait for leak threshold
	time.Sleep(time.Millisecond * 15)

	// Now should detect leak
	leaks = tracker.CheckForLeaks()
	assert.Len(t, leaks, 1)
	assert.Equal(t, ptr, leaks[0])
}

func TestNewMemoryManager(t *testing.T) {
	runtime := &Runtime{} // Mock runtime
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	assert.NotNil(t, manager)
	assert.Equal(t, runtime, manager.runtime)
	assert.Equal(t, config, manager.config)
	assert.NotNil(t, manager.pool)
	assert.NotNil(t, manager.tracker)
	assert.NotNil(t, manager.views)
}

func TestMemoryManager_BufferOperations(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	// Test buffer allocation and deallocation
	size := 512
	buffer := manager.AllocateBuffer(size)
	assert.NotNil(t, buffer)
	assert.Equal(t, 0, len(buffer))
	assert.GreaterOrEqual(t, cap(buffer), size)

	// Use the buffer
	for i := 0; i < size; i++ {
		buffer = append(buffer, byte(i%256))
	}

	// Return to pool
	manager.DeallocateBuffer(buffer)
}

func TestMemoryManager_CreateReleaseView(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	ptr := uint32(0x1000)
	size := uint32(256)

	// Create view
	view, err := manager.CreateView(ptr, size, true)
	require.NoError(t, err)
	assert.NotNil(t, view)
	assert.Equal(t, ptr, view.Ptr)
	assert.Equal(t, size, view.Size)
	assert.True(t, view.ReadOnly)
	assert.Equal(t, int32(1), view.RefCount.Load())

	// Release view
	manager.ReleaseView(ptr)

	// View should be removed
	manager.mu.RLock()
	_, exists := manager.views[ptr]
	manager.mu.RUnlock()
	assert.False(t, exists)
}

func TestMemoryManager_ViewLimits(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	config.MaxViews = 2 // Limit to 2 views for testing
	manager := NewMemoryManager(runtime, config)

	// Create maximum number of views
	view1, err := manager.CreateView(0x1000, 256, false)
	require.NoError(t, err)
	assert.NotNil(t, view1)

	view2, err := manager.CreateView(0x2000, 256, false)
	require.NoError(t, err)
	assert.NotNil(t, view2)

	// Try to create one more (should fail)
	view3, err := manager.CreateView(0x3000, 256, false)
	assert.Error(t, err)
	assert.Nil(t, view3)
	assert.Contains(t, err.Error(), "maximum number of views reached")
}

func TestMemoryView_RefCounting(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	ptr := uint32(0x1000)
	size := uint32(256)

	view, err := manager.CreateView(ptr, size, false)
	require.NoError(t, err)

	// Initial ref count should be 1
	assert.Equal(t, int32(1), view.RefCount.Load())

	// Add reference
	view.AddRef()
	assert.Equal(t, int32(2), view.RefCount.Load())

	// Access view
	assert.True(t, view.Access())

	// Release one reference
	released := view.Release()
	assert.False(t, released) // Still has 1 reference
	assert.Equal(t, int32(1), view.RefCount.Load())

	// Release final reference
	released = view.Release()
	assert.True(t, released) // Should be released
	assert.Equal(t, int32(0), view.RefCount.Load())

	// Access after release should fail
	assert.False(t, view.Access())
}

func TestMemoryView_Expiration(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	ptr := uint32(0x1000)
	size := uint32(256)

	view, err := manager.CreateView(ptr, size, false)
	require.NoError(t, err)

	// Initially not expired
	assert.False(t, view.IsExpired(time.Minute))

	// Should be expired with very short timeout
	assert.True(t, view.IsExpired(time.Nanosecond))
}

func TestMemoryManager_Cleanup(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	// Create some views
	manager.CreateView(0x1000, 256, false)
	manager.CreateView(0x2000, 256, true)

	// Verify views exist
	info := manager.GetMemoryInfo()
	assert.Equal(t, 2, info["active_views"])

	// Cleanup
	err := manager.Cleanup()
	assert.NoError(t, err)

	// Verify views are cleared
	info = manager.GetMemoryInfo()
	assert.Equal(t, 0, info["active_views"])
}

func TestMemoryManager_Concurrency(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	const numGoroutines = 10
	const operationsPerGoroutine = 100

	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	// Concurrent buffer operations
	for i := 0; i < numGoroutines; i++ {
		go func(id int) {
			defer wg.Done()

			for j := 0; j < operationsPerGoroutine; j++ {
				// Allocate buffer
				buffer := manager.AllocateBuffer(256)

				// Use buffer
				for k := 0; k < 100; k++ {
					buffer = append(buffer, byte(k))
				}

				// Return buffer
				manager.DeallocateBuffer(buffer)

				// Create and release view
				ptr := uint32(0x1000 + id*0x1000 + j)
				view, err := manager.CreateView(ptr, 128, false)
				if err == nil {
					view.Access()
					manager.ReleaseView(ptr)
				}
			}
		}(i)
	}

	wg.Wait()

	// Verify no panics occurred and system is in good state
	info := manager.GetMemoryInfo()
	assert.NotNil(t, info)
}

func TestMemoryRegion_String(t *testing.T) {
	region := &MemoryRegion{
		Ptr:         0x1000,
		Size:        256,
		Used:        true,
		Tag:         "test_region",
		AllocatedAt: time.Now(),
	}

	str := region.String()
	assert.Contains(t, str, "0x1000")
	assert.Contains(t, str, "256")
	assert.Contains(t, str, "true")
	assert.Contains(t, str, "test_region")
}

func TestMemoryManager_GetMemoryInfo(t *testing.T) {
	runtime := &Runtime{}
	config := DefaultMemoryConfig()
	manager := NewMemoryManager(runtime, config)

	// Track some allocations
	if manager.tracker != nil {
		manager.tracker.TrackAllocation(0x1000, 256, "test1")
		manager.tracker.TrackAllocation(0x2000, 512, "test2")
	}

	// Create some views
	manager.CreateView(0x3000, 128, false)
	manager.CreateView(0x4000, 256, true)

	info := manager.GetMemoryInfo()
	assert.NotNil(t, info)

	if poolStats, ok := info["pool_stats"]; ok {
		assert.NotNil(t, poolStats)
	}

	if trackingStats, ok := info["tracking_stats"]; ok {
		stats := trackingStats.(map[string]uint64)
		assert.Equal(t, uint64(768), stats["total_allocated"]) // 256 + 512
		assert.Equal(t, uint64(2), stats["allocation_count"])
	}

	assert.Equal(t, 2, info["active_views"])
}

func TestMemoryTracker_CallStackCapture(t *testing.T) {
	config := DefaultMemoryConfig()
	config.TrackCallStacks = true
	tracker := NewMemoryTracker(config)

	ptr := uint32(0x1000)
	size := uint32(256)
	tag := "stack_test"

	tracker.TrackAllocation(ptr, size, tag)

	// Verify call stack was captured
	tracker.mu.RLock()
	callStack, exists := tracker.callStacks[ptr]
	tracker.mu.RUnlock()

	assert.True(t, exists)
	assert.NotEmpty(t, callStack)
}

func TestMemoryTracker_PeakMemoryTracking(t *testing.T) {
	config := DefaultMemoryConfig()
	tracker := NewMemoryTracker(config)

	// Allocate increasing sizes
	sizes := []uint32{100, 200, 150, 300, 50}
	expectedPeak := uint32(800) // 100 + 200 + 150 + 300 + 50 = 800 (peak after all allocations)

	ptrs := make([]uint32, len(sizes))
	for i, size := range sizes {
		ptrs[i] = uint32(0x1000 + i*0x1000)
		tracker.TrackAllocation(ptrs[i], size, "peak_test")
	}

	// Check peak after all allocations
	stats := tracker.GetStats()
	assert.Equal(t, uint64(expectedPeak), stats["peak_allocated"])

	// Deallocate one
	tracker.TrackDeallocation(ptrs[4]) // Remove the 50-byte allocation

	// Peak should remain the same after deallocation
	stats = tracker.GetStats()
	assert.Equal(t, uint64(expectedPeak), stats["peak_allocated"])
}
