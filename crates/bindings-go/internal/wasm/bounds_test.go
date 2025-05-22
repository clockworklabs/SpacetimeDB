package wasm

import (
	"fmt"
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewMemoryRegionManager(t *testing.T) {
	mrm := NewMemoryRegionManager()

	assert.NotNil(t, mrm)
	assert.NotNil(t, mrm.regions)
	assert.NotNil(t, mrm.sortedRegions)
	assert.True(t, mrm.enableOverlapCheck)
	assert.False(t, mrm.enableBoundsLogging)
	assert.Equal(t, uint32(10000), mrm.maxRegions)
	assert.Equal(t, uint32(4), mrm.defaultAlignment)
}

func TestMemoryRegionManager_AddRegion(t *testing.T) {
	mrm := NewMemoryRegionManager()

	tests := []struct {
		name        string
		startAddr   uint32
		size        uint32
		tag         string
		permissions RegionPermissions
		wantErr     bool
		errMsg      string
	}{
		{
			name:      "valid region",
			startAddr: 0x1000,
			size:      256,
			tag:       "test_region",
			permissions: RegionPermissions{
				Read:  true,
				Write: true,
			},
			wantErr: false,
		},
		{
			name:      "zero size region",
			startAddr: 0x2000,
			size:      0,
			tag:       "zero_region",
			permissions: RegionPermissions{
				Read: true,
			},
			wantErr: false,
		},
		{
			name:      "region with overflow",
			startAddr: 0xFFFFFFFF,
			size:      2,
			tag:       "overflow_region",
			permissions: RegionPermissions{
				Read: true,
			},
			wantErr: true,
			errMsg:  "address overflow",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := mrm.AddRegion(tt.startAddr, tt.size, tt.tag, tt.permissions)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)

				// Verify region was added
				// Note: Zero-size regions cannot be found with FindRegion since they contain no addresses
				if tt.size > 0 {
					region := mrm.FindRegion(tt.startAddr)
					assert.NotNil(t, region)
					assert.Equal(t, tt.startAddr, region.StartAddress)
					assert.Equal(t, tt.size, region.Size)
					assert.Equal(t, tt.tag, region.Tag)
					assert.Equal(t, tt.permissions, region.Permissions)
				} else {
					// For zero-size regions, check the regions map directly
					mrm.mu.RLock()
					region, exists := mrm.regions[tt.startAddr]
					mrm.mu.RUnlock()
					assert.True(t, exists)
					assert.Equal(t, tt.startAddr, region.StartAddress)
					assert.Equal(t, tt.size, region.Size)
					assert.Equal(t, tt.tag, region.Tag)
					assert.Equal(t, tt.permissions, region.Permissions)
				}
			}
		})
	}
}

func TestMemoryRegionManager_AddRegion_Overlap(t *testing.T) {
	mrm := NewMemoryRegionManager()

	// Add first region
	err := mrm.AddRegion(0x1000, 256, "region1", RegionPermissions{Read: true})
	require.NoError(t, err)

	// Try to add overlapping region
	err = mrm.AddRegion(0x1080, 256, "region2", RegionPermissions{Read: true})
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "overlaps with region")

	// Add non-overlapping region (should succeed)
	err = mrm.AddRegion(0x2000, 256, "region3", RegionPermissions{Read: true})
	assert.NoError(t, err)
}

func TestMemoryRegionManager_RemoveRegion(t *testing.T) {
	mrm := NewMemoryRegionManager()

	startAddr := uint32(0x1000)
	size := uint32(256)

	// Add region
	err := mrm.AddRegion(startAddr, size, "test", RegionPermissions{Read: true})
	require.NoError(t, err)

	// Verify region exists
	region := mrm.FindRegion(startAddr)
	assert.NotNil(t, region)

	// Remove region
	err = mrm.RemoveRegion(startAddr)
	assert.NoError(t, err)

	// Verify region is gone
	region = mrm.FindRegion(startAddr)
	assert.Nil(t, region)

	// Try to remove again (should fail)
	err = mrm.RemoveRegion(startAddr)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "region not found")
}

func TestMemoryRegionManager_FindRegion(t *testing.T) {
	mrm := NewMemoryRegionManager()

	// Add multiple regions
	regions := []struct {
		start uint32
		size  uint32
		tag   string
	}{
		{0x1000, 256, "region1"},
		{0x2000, 512, "region2"},
		{0x3000, 128, "region3"},
	}

	for _, r := range regions {
		err := mrm.AddRegion(r.start, r.size, r.tag, RegionPermissions{Read: true})
		require.NoError(t, err)
	}

	tests := []struct {
		name    string
		address uint32
		wantTag string
		found   bool
	}{
		{
			name:    "find region1 start",
			address: 0x1000,
			wantTag: "region1",
			found:   true,
		},
		{
			name:    "find region1 middle",
			address: 0x1080,
			wantTag: "region1",
			found:   true,
		},
		{
			name:    "find region1 end",
			address: 0x10FF,
			wantTag: "region1",
			found:   true,
		},
		{
			name:    "find region2",
			address: 0x2100,
			wantTag: "region2",
			found:   true,
		},
		{
			name:    "address not in any region",
			address: 0x1500,
			found:   false,
		},
		{
			name:    "address before all regions",
			address: 0x500,
			found:   false,
		},
		{
			name:    "address after all regions",
			address: 0x5000,
			found:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			region := mrm.FindRegion(tt.address)

			if tt.found {
				assert.NotNil(t, region)
				assert.Equal(t, tt.wantTag, region.Tag)
				assert.GreaterOrEqual(t, tt.address, region.StartAddress)
				assert.Less(t, tt.address, region.EndAddress)
			} else {
				assert.Nil(t, region)
			}
		})
	}
}

func TestNewBoundsChecker(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	assert.NotNil(t, bc)
	assert.Equal(t, runtime, bc.runtime)
	assert.NotNil(t, bc.regionManager)
	assert.NotNil(t, bc.overflowDetector)
	assert.True(t, bc.enableStrictMode)
	assert.True(t, bc.enableOverflowCheck)
	assert.True(t, bc.enableUnderflowCheck)
	assert.False(t, bc.logViolations)
}

func TestNewOverflowDetector(t *testing.T) {
	od := NewOverflowDetector()

	assert.NotNil(t, od)
	assert.Greater(t, od.additionThreshold, uint32(0))
	assert.Greater(t, od.multiplicationLimit, uint32(0))
	assert.Greater(t, od.sizeCheckThreshold, uint32(0))
}

func TestBoundsChecker_CheckBounds(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Disable WASM memory checks for testing since we don't have actual memory
	bc.enableStrictMode = false

	tests := []struct {
		name      string
		address   uint32
		size      uint32
		operation string
		wantErr   bool
		errType   string
	}{
		{
			name:      "arithmetic overflow",
			address:   0xFFFFFFFF,
			size:      2,
			operation: "read",
			wantErr:   true,
			errType:   "arithmetic_overflow",
		},
		{
			name:      "large size",
			address:   0x1000,
			size:      2000000000, // 2GB
			operation: "write",
			wantErr:   true,
			errType:   "large_size",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := bc.CheckBounds(tt.address, tt.size, tt.operation)

			if tt.wantErr {
				assert.Error(t, err)
				if tt.errType != "" {
					boundsErr, ok := err.(*BoundsError)
					assert.True(t, ok)
					assert.Equal(t, tt.errType, boundsErr.Type)
				}

				// Verify statistics were updated
				stats := bc.GetBoundsStats()
				assert.Greater(t, stats["violations_found"], uint64(0))
			} else {
				assert.NoError(t, err)
			}

			// Verify checks were counted
			stats := bc.GetBoundsStats()
			assert.Greater(t, stats["checks_performed"], uint64(0))
		})
	}
}

func TestBoundsChecker_UnderflowDetection(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Test underflow detection directly
	err := bc.checkUnderflow(1, 10)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "potential_underflow")

	// Test normal case
	err = bc.checkUnderflow(0x1000, 10)
	assert.NoError(t, err)
}

func TestBoundsChecker_RegisterRegion(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	startAddr := uint32(0x1000)
	size := uint32(256)
	tag := "test_region"

	err := bc.RegisterRegion(startAddr, size, tag, true, true, false)
	assert.NoError(t, err)

	// Verify region was registered
	region := bc.GetRegion(startAddr)
	assert.NotNil(t, region)
	assert.Equal(t, startAddr, region.StartAddress)
	assert.Equal(t, size, region.Size)
	assert.Equal(t, tag, region.Tag)
	assert.True(t, region.Permissions.Read)
	assert.True(t, region.Permissions.Write)
	assert.False(t, region.Permissions.Execute)
}

func TestBoundsChecker_StrictMode(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Test strict mode behavior by bypassing WASM memory checks
	// We'll test the region-based checks only

	// Register a region
	bc.RegisterRegion(0x1000, 256, "test", true, true, false)

	// Manually test region checking logic
	region := bc.GetRegion(0x1000)
	assert.NotNil(t, region)

	// Test unregistered region detection
	region = bc.GetRegion(0x2000)
	assert.Nil(t, region)

	// Test mode setting
	bc.SetStrictMode(false)
	assert.False(t, bc.enableStrictMode)

	bc.SetStrictMode(true)
	assert.True(t, bc.enableStrictMode)
}

func TestBoundsChecker_RegionPermissions(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Register read-only region
	bc.RegisterRegion(0x1000, 256, "readonly", true, false, false)

	// Register executable region
	bc.RegisterRegion(0x2000, 256, "executable", true, true, true)

	// Test permissions directly on regions
	region1 := bc.GetRegion(0x1000)
	assert.NotNil(t, region1)
	assert.True(t, region1.Permissions.Read)
	assert.False(t, region1.Permissions.Write)
	assert.False(t, region1.Permissions.Execute)

	region2 := bc.GetRegion(0x2000)
	assert.NotNil(t, region2)
	assert.True(t, region2.Permissions.Read)
	assert.True(t, region2.Permissions.Write)
	assert.True(t, region2.Permissions.Execute)

	// Test permission checking logic directly
	err := bc.checkRegionPermissions(region1, "read")
	assert.NoError(t, err)

	err = bc.checkRegionPermissions(region1, "write")
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "write permission denied")

	err = bc.checkRegionPermissions(region1, "execute")
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "execute permission denied")
}

func TestBoundsChecker_ValidateAlignment(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	tests := []struct {
		name      string
		address   uint32
		alignment uint32
		wantErr   bool
		errMsg    string
	}{
		{
			name:      "valid 4-byte alignment",
			address:   0x1000,
			alignment: 4,
			wantErr:   false,
		},
		{
			name:      "valid 8-byte alignment",
			address:   0x1008,
			alignment: 8,
			wantErr:   false,
		},
		{
			name:      "misaligned address",
			address:   0x1001,
			alignment: 4,
			wantErr:   true,
			errMsg:    "not aligned",
		},
		{
			name:      "invalid alignment (not power of 2)",
			address:   0x1000,
			alignment: 3,
			wantErr:   true,
			errMsg:    "alignment must be power of 2",
		},
		{
			name:      "zero alignment",
			address:   0x1000,
			alignment: 0,
			wantErr:   true,
			errMsg:    "alignment must be power of 2",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := bc.ValidateAlignment(tt.address, tt.alignment)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestOverflowDetector_CheckOverflow(t *testing.T) {
	od := NewOverflowDetector()

	tests := []struct {
		name    string
		address uint32
		size    uint32
		wantErr bool
		errType string
	}{
		{
			name:    "normal allocation",
			address: 0x1000,
			size:    1024,
			wantErr: false,
		},
		{
			name:    "near threshold address",
			address: od.additionThreshold + 1,
			size:    100,
			wantErr: true,
			errType: "addition_overflow",
		},
		{
			name:    "large size",
			address: 0x1000,
			size:    od.sizeCheckThreshold + 1,
			wantErr: true,
			errType: "large_size",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := od.CheckOverflow(tt.address, tt.size)

			if tt.wantErr {
				assert.Error(t, err)
				if tt.errType != "" {
					boundsErr, ok := err.(*BoundsError)
					assert.True(t, ok)
					assert.Equal(t, tt.errType, boundsErr.Type)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestBoundsChecker_GetBoundsStats(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Initially, stats should be zero
	stats := bc.GetBoundsStats()
	assert.Equal(t, uint64(0), stats["checks_performed"])
	assert.Equal(t, uint64(0), stats["violations_found"])

	// Perform some checks
	bc.CheckBounds(0x1000, 256, "read")
	bc.CheckBounds(0xFFFFFFFF, 2, "write") // This should cause overflow

	// Verify stats updated
	stats = bc.GetBoundsStats()
	assert.Greater(t, stats["checks_performed"], uint64(0))
	assert.Greater(t, stats["violations_found"], uint64(0))

	// Verify other stats are present
	assert.Contains(t, stats, "overflows_found")
	assert.Contains(t, stats, "underflows_found")
	assert.Contains(t, stats, "strict_mode")
	assert.Contains(t, stats, "region_stats")
	assert.Contains(t, stats, "overflow_stats")
}

func TestBoundsChecker_ListRegions(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Initially empty
	regions := bc.ListRegions()
	assert.Len(t, regions, 0)

	// Register some regions
	addresses := []uint32{0x1000, 0x2000, 0x3000}
	for i, addr := range addresses {
		bc.RegisterRegion(addr, 256, fmt.Sprintf("region%d", i), true, true, false)
	}

	// List regions
	regions = bc.ListRegions()
	assert.Len(t, regions, 3)

	// Verify regions are sorted by address
	for i := 1; i < len(regions); i++ {
		assert.Less(t, regions[i-1].StartAddress, regions[i].StartAddress)
	}
}

func TestBoundsChecker_Cleanup(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Register some regions and perform checks
	bc.RegisterRegion(0x1000, 256, "test1", true, true, false)
	bc.RegisterRegion(0x2000, 256, "test2", true, true, false)
	bc.CheckBounds(0x1000, 100, "read")
	bc.CheckBounds(0xFFFFFFFF, 2, "write") // Cause violation

	// Verify data exists
	assert.Len(t, bc.ListRegions(), 2)
	stats := bc.GetBoundsStats()
	assert.Greater(t, stats["checks_performed"], uint64(0))

	// Cleanup
	bc.Cleanup()

	// Verify everything is cleared
	assert.Len(t, bc.ListRegions(), 0)
	stats = bc.GetBoundsStats()
	assert.Equal(t, uint64(0), stats["checks_performed"])
	assert.Equal(t, uint64(0), stats["violations_found"])
}

func TestBoundsChecker_SetModes(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	// Test setting modes
	bc.SetStrictMode(false)
	bc.SetOverflowDetection(false)
	bc.SetUnderflowDetection(false)

	stats := bc.GetBoundsStats()
	assert.False(t, stats["strict_mode"].(bool))

	// Re-enable modes
	bc.SetStrictMode(true)
	bc.SetOverflowDetection(true)
	bc.SetUnderflowDetection(true)

	stats = bc.GetBoundsStats()
	assert.True(t, stats["strict_mode"].(bool))
}

func TestBoundsChecker_Concurrency(t *testing.T) {
	runtime := &Runtime{}
	bc := NewBoundsChecker(runtime)

	const numGoroutines = 10
	const operationsPerGoroutine = 100

	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	for i := 0; i < numGoroutines; i++ {
		go func(id int) {
			defer wg.Done()

			for j := 0; j < operationsPerGoroutine; j++ {
				address := uint32(0x1000 + id*0x1000 + j*16)

				// Register region
				bc.RegisterRegion(address, 16, "concurrent_test", true, true, false)

				// Check bounds
				bc.CheckBounds(address, 8, "read")
				bc.CheckBounds(address, 8, "write")

				// Unregister region
				bc.UnregisterRegion(address)
			}
		}(i)
	}

	wg.Wait()

	// Verify system is in good state
	stats := bc.GetBoundsStats()
	assert.NotNil(t, stats)
}

func TestBoundedRegion_String(t *testing.T) {
	region := &BoundedRegion{
		StartAddress: 0x1000,
		EndAddress:   0x1100,
		Size:         256,
		Tag:          "test_region",
	}

	str := region.String()
	assert.Contains(t, str, "0x1000")
	assert.Contains(t, str, "0x1100")
	assert.Contains(t, str, "256")
	assert.Contains(t, str, "test_region")
}

func TestBoundsError_Error(t *testing.T) {
	err := &BoundsError{
		Type:    "test_error",
		Address: 0x1000,
		Size:    256,
		Limit:   0x2000,
		Message: "test message",
	}

	errStr := err.Error()
	assert.Contains(t, errStr, "test_error")
	assert.Contains(t, errStr, "0x1000")
	assert.Contains(t, errStr, "256")
	assert.Contains(t, errStr, "0x2000")
	assert.Contains(t, errStr, "test message")
}
