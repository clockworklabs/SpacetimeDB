package wasm

import (
	"sync"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewPointer(t *testing.T) {
	runtime := &Runtime{}
	address := uint32(0x1000)
	size := uint32(256)
	readOnly := false

	ptr := NewPointer(runtime, address, size, readOnly)

	assert.NotNil(t, ptr)
	assert.Equal(t, address, ptr.address)
	assert.Equal(t, size, ptr.size)
	assert.Equal(t, uint32(1), ptr.alignment)
	assert.Equal(t, readOnly, ptr.readOnly)
	assert.True(t, ptr.valid.Load())
	assert.Equal(t, runtime, ptr.runtime)
}

func TestNewAlignedPointer(t *testing.T) {
	runtime := &Runtime{}

	tests := []struct {
		name      string
		address   uint32
		size      uint32
		alignment uint32
		readOnly  bool
		wantErr   bool
		errMsg    string
	}{
		{
			name:      "valid 4-byte alignment",
			address:   0x1000,
			size:      256,
			alignment: 4,
			readOnly:  false,
			wantErr:   false,
		},
		{
			name:      "valid 8-byte alignment",
			address:   0x1008,
			size:      256,
			alignment: 8,
			readOnly:  true,
			wantErr:   false,
		},
		{
			name:      "invalid alignment (not power of 2)",
			address:   0x1000,
			size:      256,
			alignment: 3,
			readOnly:  false,
			wantErr:   true,
			errMsg:    "alignment must be power of 2",
		},
		{
			name:      "zero alignment",
			address:   0x1000,
			size:      256,
			alignment: 0,
			readOnly:  false,
			wantErr:   true,
			errMsg:    "alignment must be power of 2",
		},
		{
			name:      "misaligned address",
			address:   0x1001, // Not aligned to 4 bytes
			size:      256,
			alignment: 4,
			readOnly:  false,
			wantErr:   true,
			errMsg:    "not aligned",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ptr, err := NewAlignedPointer(runtime, tt.address, tt.size, tt.alignment, tt.readOnly)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Nil(t, ptr)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, ptr)
				assert.Equal(t, tt.address, ptr.address)
				assert.Equal(t, tt.size, ptr.size)
				assert.Equal(t, tt.alignment, ptr.alignment)
				assert.Equal(t, tt.readOnly, ptr.readOnly)
			}
		})
	}
}

func TestNewPointerManager(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	assert.NotNil(t, pm)
	assert.Equal(t, runtime, pm.runtime)
	assert.NotNil(t, pm.pointers)
	assert.NotNil(t, pm.permissions)
	assert.True(t, pm.enforceAlignment)
	assert.True(t, pm.trackAccess)
	assert.True(t, pm.enableProtection)
}

func TestPointerManager_CreatePointer(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	address := uint32(0x1000)
	size := uint32(256)
	readOnly := false
	tag := "test_pointer"

	ptr, err := pm.CreatePointer(address, size, readOnly, tag)
	require.NoError(t, err)
	assert.NotNil(t, ptr)

	// Verify pointer was registered
	pm.mu.RLock()
	registeredPtr, exists := pm.pointers[address]
	pm.mu.RUnlock()

	assert.True(t, exists)
	assert.Equal(t, ptr, registeredPtr)
	assert.Equal(t, tag, ptr.tag)

	// Verify permissions were set
	perm, err := pm.GetPermissions(address)
	require.NoError(t, err)
	assert.True(t, perm.Read)
	assert.True(t, perm.Write) // Should be true since readOnly is false
	assert.False(t, perm.Execute)
}

func TestPointerManager_CreatePointer_ReadOnly(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	address := uint32(0x1000)
	size := uint32(256)
	readOnly := true
	tag := "readonly_pointer"

	ptr, err := pm.CreatePointer(address, size, readOnly, tag)
	require.NoError(t, err)
	assert.NotNil(t, ptr)
	assert.True(t, ptr.readOnly)

	// Verify permissions for read-only pointer
	perm, err := pm.GetPermissions(address)
	require.NoError(t, err)
	assert.True(t, perm.Read)
	assert.False(t, perm.Write) // Should be false for read-only
	assert.False(t, perm.Execute)
}

func TestPointerManager_CreatePointer_Overlap(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	// Create first pointer
	ptr1, err := pm.CreatePointer(0x1000, 256, false, "ptr1")
	require.NoError(t, err)
	assert.NotNil(t, ptr1)

	// Try to create overlapping pointer (should fail)
	ptr2, err := pm.CreatePointer(0x1080, 256, false, "ptr2") // Overlaps with ptr1
	assert.Error(t, err)
	assert.Nil(t, ptr2)
	assert.Contains(t, err.Error(), "overlaps with existing pointer")

	// Create non-overlapping pointer (should succeed)
	ptr3, err := pm.CreatePointer(0x2000, 256, false, "ptr3")
	require.NoError(t, err)
	assert.NotNil(t, ptr3)
}

func TestPointerManager_ReleasePointer(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	address := uint32(0x1000)
	ptr, err := pm.CreatePointer(address, 256, false, "test")
	require.NoError(t, err)

	// Release the pointer
	err = pm.ReleasePointer(address)
	assert.NoError(t, err)

	// Verify pointer is invalid
	assert.False(t, ptr.valid.Load())

	// Verify pointer is removed from manager
	pm.mu.RLock()
	_, exists := pm.pointers[address]
	pm.mu.RUnlock()
	assert.False(t, exists)

	// Try to release again (should fail)
	err = pm.ReleasePointer(address)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "pointer not found")
}

func TestPointer_SafeRead(t *testing.T) {
	// Create a runtime without memory for boundary testing
	runtime := &Runtime{}
	ptr := NewPointer(runtime, 0x1000, 256, false)

	tests := []struct {
		name    string
		offset  uint32
		size    uint32
		wantErr bool
		errMsg  string
	}{
		{
			name:    "read beyond pointer bounds",
			offset:  250,
			size:    20,
			wantErr: true,
			errMsg:  "read beyond pointer bounds",
		},
		{
			name:    "read from invalidated pointer",
			offset:  0,
			size:    10,
			wantErr: true,
			errMsg:  "pointer invalidated",
		},
	}

	for i, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Invalidate pointer for the last test
			if i == len(tests)-1 {
				ptr.Invalidate()
			}

			data, err := ptr.SafeRead(tt.offset, tt.size)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Nil(t, data)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, data)
				assert.Equal(t, int(tt.size), len(data))

				// Verify access was tracked
				assert.Greater(t, ptr.GetAccessCount(), uint64(0))
			}
		})
	}
}

func TestPointer_SafeWrite(t *testing.T) {
	runtime := &Runtime{}

	tests := []struct {
		name     string
		readOnly bool
		offset   uint32
		data     []byte
		wantErr  bool
		errMsg   string
	}{
		{
			name:     "write to read-only pointer",
			readOnly: true,
			offset:   0,
			data:     []byte{1, 2, 3, 4},
			wantErr:  true,
			errMsg:   "write to read-only pointer",
		},
		{
			name:     "write beyond bounds",
			readOnly: false,
			offset:   250,
			data:     make([]byte, 20),
			wantErr:  true,
			errMsg:   "write beyond pointer bounds",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ptr := NewPointer(runtime, 0x1000, 256, tt.readOnly)

			err := ptr.SafeWrite(tt.offset, tt.data)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)

				// Verify access was tracked
				assert.Greater(t, ptr.GetAccessCount(), uint64(0))
			}
		})
	}
}

func TestPointer_SafePointerArithmetic(t *testing.T) {
	runtime := &Runtime{}
	ptr := NewPointer(runtime, 0x1000, 256, false)

	tests := []struct {
		name     string
		offset   int32
		wantErr  bool
		errMsg   string
		wantAddr uint32
		wantSize uint32
	}{
		{
			name:     "positive offset",
			offset:   100,
			wantErr:  false,
			wantAddr: 0x1064, // 0x1000 + 100
			wantSize: 156,    // 256 - 100
		},
		{
			name:    "negative offset",
			offset:  -50,
			wantErr: true,
			errMsg:  "pointer arithmetic before start",
		},
		{
			name:    "offset beyond bounds",
			offset:  300,
			wantErr: true,
			errMsg:  "pointer arithmetic beyond bounds",
		},
		{
			name:     "zero offset",
			offset:   0,
			wantErr:  false,
			wantAddr: 0x1000,
			wantSize: 256,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			newPtr, err := ptr.SafePointerArithmetic(tt.offset)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Nil(t, newPtr)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, newPtr)
				assert.Equal(t, tt.wantAddr, newPtr.address)
				assert.Equal(t, tt.wantSize, newPtr.size)
				assert.Equal(t, ptr.alignment, newPtr.alignment)
				assert.Equal(t, ptr.readOnly, newPtr.readOnly)
				assert.Equal(t, ptr.tag, newPtr.tag)
			}
		})
	}
}

func TestPointer_AddSubOffset(t *testing.T) {
	runtime := &Runtime{}
	ptr := NewPointer(runtime, 0x1000, 256, false)

	// Test AddOffset
	newPtr, err := ptr.AddOffset(50)
	require.NoError(t, err)
	assert.Equal(t, uint32(0x1032), newPtr.address) // 0x1000 + 50
	assert.Equal(t, uint32(206), newPtr.size)       // 256 - 50

	// Test SubOffset (should fail since we can't go before start)
	_, err = ptr.SubOffset(50)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "before start")
}

func TestPointer_Properties(t *testing.T) {
	runtime := &Runtime{}
	ptr := NewPointer(runtime, 0x1000, 256, true)
	ptr.tag = "test_pointer"

	assert.Equal(t, uint32(0x1000), ptr.GetAddress())
	assert.Equal(t, uint32(256), ptr.GetSize())
	assert.Equal(t, uint32(1), ptr.GetAlignment())
	assert.True(t, ptr.IsReadOnly())
	assert.True(t, ptr.IsValid())
	assert.False(t, ptr.IsNull())
	assert.True(t, ptr.IsAligned())
	assert.Equal(t, "test_pointer", ptr.GetTag())

	// Test null pointer
	nullPtr := NewPointer(runtime, 0, 256, false)
	assert.True(t, nullPtr.IsNull())

	// Test invalidation
	ptr.Invalidate()
	assert.False(t, ptr.IsValid())
}

func TestPointerManager_ValidatePointer(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	// Create a registered pointer
	_, err := pm.CreatePointer(0x1000, 256, false, "test")
	require.NoError(t, err)

	tests := []struct {
		name       string
		address    uint32
		accessSize uint32
		write      bool
		wantErr    bool
		errMsg     string
	}{
		{
			name:       "access beyond bounds",
			address:    0x1000,
			accessSize: 300,
			write:      false,
			wantErr:    true,
			errMsg:     "access outside pointer bounds",
		},
		{
			name:       "unregistered pointer access",
			address:    0x2000,
			accessSize: 100,
			write:      false,
			wantErr:    true,
			errMsg:     "unregistered pointer access",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := pm.ValidatePointer(tt.address, tt.accessSize, tt.write)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestPointerManager_SetGetPermissions(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	address := uint32(0x1000)
	_, err := pm.CreatePointer(address, 256, false, "test")
	require.NoError(t, err)

	// Get default permissions
	perm, err := pm.GetPermissions(address)
	require.NoError(t, err)
	assert.True(t, perm.Read)
	assert.True(t, perm.Write)
	assert.False(t, perm.Execute)

	// Set new permissions
	newPerm := PointerPermission{
		Read:    true,
		Write:   false,
		Execute: true,
		Region: MemoryRegion{
			Ptr:  address,
			Size: 256,
			Used: true,
			Tag:  "test",
		},
	}

	err = pm.SetPermissions(address, newPerm)
	require.NoError(t, err)

	// Verify new permissions
	perm, err = pm.GetPermissions(address)
	require.NoError(t, err)
	assert.True(t, perm.Read)
	assert.False(t, perm.Write)
	assert.True(t, perm.Execute)

	// Test validation with new permissions
	err = pm.ValidatePointer(address, 100, true) // Write access should fail
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "write access denied")
}

func TestPointerManager_Concurrency(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	const numGoroutines = 10
	const operationsPerGoroutine = 100

	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	for i := 0; i < numGoroutines; i++ {
		go func(id int) {
			defer wg.Done()

			for j := 0; j < operationsPerGoroutine; j++ {
				address := uint32(0x1000 + id*0x1000 + j*16)

				// Create pointer
				_, err := pm.CreatePointer(address, 16, false, "concurrent_test")
				if err != nil {
					continue // Skip if overlapping
				}

				// Validate access
				pm.ValidatePointer(address, 8, false)
				pm.ValidatePointer(address, 8, true)

				// Release pointer
				pm.ReleasePointer(address)
			}
		}(i)
	}

	wg.Wait()

	// Verify system is in good state
	stats := pm.GetPointerStats()
	assert.NotNil(t, stats)
}

func TestPointerManager_GetPointerStats(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	// Create some pointers
	pm.CreatePointer(0x1000, 256, false, "ptr1")
	pm.CreatePointer(0x2000, 256, true, "ptr2")
	pm.CreatePointer(0x3000, 256, false, "ptr3")

	stats := pm.GetPointerStats()
	assert.Equal(t, 3, stats["total_pointers"])
	assert.Equal(t, 3, stats["valid_pointers"])
	assert.Equal(t, 1, stats["readonly_pointers"])
	assert.Equal(t, 3, stats["permissions_count"])
}

func TestPointerManager_ListPointers(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	// Initially empty
	pointers := pm.ListPointers()
	assert.Len(t, pointers, 0)

	// Create some pointers
	addresses := []uint32{0x1000, 0x2000, 0x3000}
	for _, addr := range addresses {
		pm.CreatePointer(addr, 256, false, "test")
	}

	// List pointers
	pointers = pm.ListPointers()
	assert.Len(t, pointers, 3)

	for _, addr := range addresses {
		ptr, exists := pointers[addr]
		assert.True(t, exists)
		assert.Equal(t, addr, ptr.address)
	}
}

func TestPointerManager_Cleanup(t *testing.T) {
	runtime := &Runtime{}
	pm := NewPointerManager(runtime)

	// Create some pointers
	pm.CreatePointer(0x1000, 256, false, "ptr1")
	pm.CreatePointer(0x2000, 256, false, "ptr2")

	// Verify pointers exist
	assert.Len(t, pm.ListPointers(), 2)

	// Cleanup
	pm.Cleanup()

	// Verify pointers are cleared and invalidated
	assert.Len(t, pm.ListPointers(), 0)
}

func TestPointer_String(t *testing.T) {
	runtime := &Runtime{}
	ptr := NewPointer(runtime, 0x1000, 256, true)
	ptr.tag = "test_pointer"

	str := ptr.String()
	assert.Contains(t, str, "0x1000")
	assert.Contains(t, str, "256")
	assert.Contains(t, str, "true")
	assert.Contains(t, str, "test_pointer")
}
