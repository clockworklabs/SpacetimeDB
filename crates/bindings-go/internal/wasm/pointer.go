package wasm

import (
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

// PointerError represents pointer-related errors
type PointerError struct {
	Type    string
	Pointer uint32
	Size    uint32
	Message string
}

func (e *PointerError) Error() string {
	return fmt.Sprintf("pointer error [%s]: %s (ptr=0x%x, size=%d)", e.Type, e.Message, e.Pointer, e.Size)
}

// Pointer represents a safe WASM memory pointer with bounds checking
type Pointer struct {
	address   uint32
	size      uint32
	alignment uint32
	readOnly  bool
	valid     atomic.Bool
	runtime   *Runtime

	// Access tracking
	accessCount atomic.Uint64
	lastAccess  atomic.Uint64 // Unix timestamp

	// Protection flags
	protected bool
	tag       string
}

// PointerManager manages safe pointer operations
type PointerManager struct {
	runtime     *Runtime
	pointers    map[uint32]*Pointer
	permissions map[uint32]PointerPermission
	mu          sync.RWMutex

	// Configuration
	enforceAlignment bool
	trackAccess      bool
	enableProtection bool
}

// PointerPermission defines access permissions for memory regions
type PointerPermission struct {
	Read    bool
	Write   bool
	Execute bool
	Region  MemoryRegion
}

// NewPointer creates a new safe pointer
func NewPointer(runtime *Runtime, address uint32, size uint32, readOnly bool) *Pointer {
	ptr := &Pointer{
		address:   address,
		size:      size,
		alignment: 1, // Default byte alignment
		readOnly:  readOnly,
		runtime:   runtime,
	}
	ptr.valid.Store(true)
	return ptr
}

// NewAlignedPointer creates a new pointer with specific alignment requirements
func NewAlignedPointer(runtime *Runtime, address uint32, size uint32, alignment uint32, readOnly bool) (*Pointer, error) {
	if alignment == 0 || (alignment&(alignment-1)) != 0 {
		return nil, &PointerError{
			Type:    "alignment",
			Pointer: address,
			Size:    size,
			Message: fmt.Sprintf("alignment must be power of 2, got %d", alignment),
		}
	}

	if address%alignment != 0 {
		return nil, &PointerError{
			Type:    "alignment",
			Pointer: address,
			Size:    size,
			Message: fmt.Sprintf("address 0x%x not aligned to %d bytes", address, alignment),
		}
	}

	ptr := NewPointer(runtime, address, size, readOnly)
	ptr.alignment = alignment
	return ptr, nil
}

// NewPointerManager creates a new pointer manager
func NewPointerManager(runtime *Runtime) *PointerManager {
	return &PointerManager{
		runtime:          runtime,
		pointers:         make(map[uint32]*Pointer),
		permissions:      make(map[uint32]PointerPermission),
		enforceAlignment: true,
		trackAccess:      true,
		enableProtection: true,
	}
}

// CreatePointer creates and registers a new safe pointer
func (pm *PointerManager) CreatePointer(address uint32, size uint32, readOnly bool, tag string) (*Pointer, error) {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	// Check for overlapping pointers
	if err := pm.checkOverlap(address, size); err != nil {
		return nil, err
	}

	ptr := NewPointer(pm.runtime, address, size, readOnly)
	ptr.tag = tag
	pm.pointers[address] = ptr

	// Set default permissions
	pm.permissions[address] = PointerPermission{
		Read:    true,
		Write:   !readOnly,
		Execute: false,
		Region: MemoryRegion{
			Ptr:  address,
			Size: size,
			Used: true,
			Tag:  tag,
		},
	}

	return ptr, nil
}

// ReleasePointer releases a pointer and removes it from management
func (pm *PointerManager) ReleasePointer(address uint32) error {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	ptr, exists := pm.pointers[address]
	if !exists {
		return &PointerError{
			Type:    "release",
			Pointer: address,
			Message: "pointer not found",
		}
	}

	ptr.valid.Store(false)
	delete(pm.pointers, address)
	delete(pm.permissions, address)

	return nil
}

// ValidatePointer checks if a pointer is valid and within bounds
func (pm *PointerManager) ValidatePointer(address uint32, accessSize uint32, write bool) error {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	// Check if we have a registered pointer for this address
	ptr, exists := pm.pointers[address]
	if !exists {
		// Check if address falls within any existing pointer's range
		for _, p := range pm.pointers {
			if address >= p.address && address < p.address+p.size {
				ptr = p
				exists = true
				break
			}
		}
	}

	if !exists && pm.enableProtection {
		return &PointerError{
			Type:    "validation",
			Pointer: address,
			Size:    accessSize,
			Message: "unregistered pointer access",
		}
	}

	if exists {
		return pm.validatePointerAccess(ptr, address, accessSize, write)
	}

	// Fallback to basic bounds checking against WASM memory
	return pm.validateMemoryBounds(address, accessSize)
}

// validatePointerAccess validates access to a registered pointer
func (pm *PointerManager) validatePointerAccess(ptr *Pointer, address uint32, accessSize uint32, write bool) error {
	if !ptr.valid.Load() {
		return &PointerError{
			Type:    "validation",
			Pointer: address,
			Size:    accessSize,
			Message: "pointer has been invalidated",
		}
	}

	// Check bounds
	if address < ptr.address || address+accessSize > ptr.address+ptr.size {
		return &PointerError{
			Type:    "bounds",
			Pointer: address,
			Size:    accessSize,
			Message: fmt.Sprintf("access outside pointer bounds [0x%x-0x%x]", ptr.address, ptr.address+ptr.size),
		}
	}

	// Check permissions
	perm, exists := pm.permissions[ptr.address]
	if exists {
		if write && (!perm.Write || ptr.readOnly) {
			return &PointerError{
				Type:    "permission",
				Pointer: address,
				Size:    accessSize,
				Message: "write access denied",
			}
		}
		if !perm.Read {
			return &PointerError{
				Type:    "permission",
				Pointer: address,
				Size:    accessSize,
				Message: "read access denied",
			}
		}
	}

	// Check alignment if enforced
	if pm.enforceAlignment && address%ptr.alignment != 0 {
		return &PointerError{
			Type:    "alignment",
			Pointer: address,
			Size:    accessSize,
			Message: fmt.Sprintf("unaligned access (required alignment: %d)", ptr.alignment),
		}
	}

	// Track access if enabled
	if pm.trackAccess {
		ptr.accessCount.Add(1)
		ptr.lastAccess.Store(uint64(getCurrentTimestamp()))
	}

	return nil
}

// validateMemoryBounds validates basic memory bounds against WASM memory
func (pm *PointerManager) validateMemoryBounds(address uint32, size uint32) error {
	if pm.runtime.memory == nil {
		return &PointerError{
			Type:    "memory",
			Pointer: address,
			Size:    size,
			Message: "WASM memory not initialized",
		}
	}

	memorySize := pm.runtime.memory.Size()
	if address+size > memorySize {
		return &PointerError{
			Type:    "bounds",
			Pointer: address,
			Size:    size,
			Message: fmt.Sprintf("access beyond memory size (memory: %d bytes)", memorySize),
		}
	}

	return nil
}

// checkOverlap checks for overlapping pointer regions
func (pm *PointerManager) checkOverlap(address uint32, size uint32) error {
	for addr, ptr := range pm.pointers {
		// Check if ranges overlap
		if !(address+size <= ptr.address || address >= ptr.address+ptr.size) {
			return &PointerError{
				Type:    "overlap",
				Pointer: address,
				Size:    size,
				Message: fmt.Sprintf("overlaps with existing pointer at 0x%x (size: %d)", addr, ptr.size),
			}
		}
	}
	return nil
}

// SafeRead performs a safe read operation through a pointer
func (ptr *Pointer) SafeRead(offset uint32, size uint32) ([]byte, error) {
	if !ptr.valid.Load() {
		return nil, &PointerError{
			Type:    "read",
			Pointer: ptr.address + offset,
			Size:    size,
			Message: "pointer invalidated",
		}
	}

	readAddr := ptr.address + offset
	if offset+size > ptr.size {
		return nil, &PointerError{
			Type:    "bounds",
			Pointer: readAddr,
			Size:    size,
			Message: "read beyond pointer bounds",
		}
	}

	data, err := ptr.runtime.ReadFromMemory(readAddr, size)
	if err != nil {
		return nil, &PointerError{
			Type:    "read",
			Pointer: readAddr,
			Size:    size,
			Message: fmt.Sprintf("runtime read failed: %v", err),
		}
	}

	// Track access
	ptr.accessCount.Add(1)
	ptr.lastAccess.Store(uint64(getCurrentTimestamp()))

	return data, nil
}

// SafeWrite performs a safe write operation through a pointer
func (ptr *Pointer) SafeWrite(offset uint32, data []byte) error {
	if !ptr.valid.Load() {
		return &PointerError{
			Type:    "write",
			Pointer: ptr.address + offset,
			Size:    uint32(len(data)),
			Message: "pointer invalidated",
		}
	}

	if ptr.readOnly {
		return &PointerError{
			Type:    "write",
			Pointer: ptr.address + offset,
			Size:    uint32(len(data)),
			Message: "write to read-only pointer",
		}
	}

	writeAddr := ptr.address + offset
	if offset+uint32(len(data)) > ptr.size {
		return &PointerError{
			Type:    "bounds",
			Pointer: writeAddr,
			Size:    uint32(len(data)),
			Message: "write beyond pointer bounds",
		}
	}

	err := ptr.runtime.WriteToMemoryAt(writeAddr, data)
	if err != nil {
		return &PointerError{
			Type:    "write",
			Pointer: writeAddr,
			Size:    uint32(len(data)),
			Message: fmt.Sprintf("runtime write failed: %v", err),
		}
	}

	// Track access
	ptr.accessCount.Add(1)
	ptr.lastAccess.Store(uint64(getCurrentTimestamp()))

	return nil
}

// SafePointerArithmetic performs safe pointer arithmetic
func (ptr *Pointer) SafePointerArithmetic(offset int32) (*Pointer, error) {
	if !ptr.valid.Load() {
		return nil, &PointerError{
			Type:    "arithmetic",
			Pointer: ptr.address,
			Message: "pointer invalidated",
		}
	}

	newAddr := int64(ptr.address) + int64(offset)
	if newAddr < 0 {
		return nil, &PointerError{
			Type:    "arithmetic",
			Pointer: ptr.address,
			Message: "pointer arithmetic resulted in negative address",
		}
	}

	newAddrU32 := uint32(newAddr)
	if offset > 0 {
		// Moving forward - check bounds
		if newAddrU32 >= ptr.address+ptr.size {
			return nil, &PointerError{
				Type:    "arithmetic",
				Pointer: newAddrU32,
				Message: "pointer arithmetic beyond bounds",
			}
		}
	} else {
		// Moving backward - check bounds
		if newAddrU32 < ptr.address {
			return nil, &PointerError{
				Type:    "arithmetic",
				Pointer: newAddrU32,
				Message: "pointer arithmetic before start",
			}
		}
	}

	// Calculate remaining size
	remainingSize := ptr.size - (newAddrU32 - ptr.address)

	// Create new pointer with adjusted bounds
	newPtr := NewPointer(ptr.runtime, newAddrU32, remainingSize, ptr.readOnly)
	newPtr.alignment = ptr.alignment
	newPtr.protected = ptr.protected
	newPtr.tag = ptr.tag

	return newPtr, nil
}

// AddOffset adds an offset to the pointer (safe arithmetic)
func (ptr *Pointer) AddOffset(offset uint32) (*Pointer, error) {
	return ptr.SafePointerArithmetic(int32(offset))
}

// SubOffset subtracts an offset from the pointer (safe arithmetic)
func (ptr *Pointer) SubOffset(offset uint32) (*Pointer, error) {
	return ptr.SafePointerArithmetic(-int32(offset))
}

// IsNull checks if the pointer is null (address 0)
func (ptr *Pointer) IsNull() bool {
	return ptr.address == 0
}

// IsValid checks if the pointer is valid
func (ptr *Pointer) IsValid() bool {
	return ptr.valid.Load()
}

// IsAligned checks if the pointer is properly aligned
func (ptr *Pointer) IsAligned() bool {
	return ptr.address%ptr.alignment == 0
}

// GetAddress returns the pointer's address
func (ptr *Pointer) GetAddress() uint32 {
	return ptr.address
}

// GetSize returns the pointer's size
func (ptr *Pointer) GetSize() uint32 {
	return ptr.size
}

// GetAlignment returns the pointer's alignment requirement
func (ptr *Pointer) GetAlignment() uint32 {
	return ptr.alignment
}

// IsReadOnly returns whether the pointer is read-only
func (ptr *Pointer) IsReadOnly() bool {
	return ptr.readOnly
}

// GetAccessCount returns the number of times this pointer has been accessed
func (ptr *Pointer) GetAccessCount() uint64 {
	return ptr.accessCount.Load()
}

// GetLastAccess returns the timestamp of the last access
func (ptr *Pointer) GetLastAccess() uint64 {
	return ptr.lastAccess.Load()
}

// GetTag returns the pointer's tag
func (ptr *Pointer) GetTag() string {
	return ptr.tag
}

// Invalidate marks the pointer as invalid
func (ptr *Pointer) Invalidate() {
	ptr.valid.Store(false)
}

// SetPermissions sets access permissions for a pointer region
func (pm *PointerManager) SetPermissions(address uint32, perm PointerPermission) error {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	if _, exists := pm.pointers[address]; !exists {
		return &PointerError{
			Type:    "permission",
			Pointer: address,
			Message: "pointer not found",
		}
	}

	pm.permissions[address] = perm
	return nil
}

// GetPermissions returns access permissions for a pointer region
func (pm *PointerManager) GetPermissions(address uint32) (PointerPermission, error) {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	perm, exists := pm.permissions[address]
	if !exists {
		return PointerPermission{}, &PointerError{
			Type:    "permission",
			Pointer: address,
			Message: "no permissions found for pointer",
		}
	}

	return perm, nil
}

// ListPointers returns all managed pointers
func (pm *PointerManager) ListPointers() map[uint32]*Pointer {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	result := make(map[uint32]*Pointer)
	for addr, ptr := range pm.pointers {
		result[addr] = ptr
	}
	return result
}

// GetPointerStats returns statistics about pointer usage
func (pm *PointerManager) GetPointerStats() map[string]interface{} {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	totalAccess := uint64(0)
	validPointers := 0
	readOnlyPointers := 0

	for _, ptr := range pm.pointers {
		if ptr.valid.Load() {
			validPointers++
			if ptr.readOnly {
				readOnlyPointers++
			}
			totalAccess += ptr.accessCount.Load()
		}
	}

	return map[string]interface{}{
		"total_pointers":     len(pm.pointers),
		"valid_pointers":     validPointers,
		"readonly_pointers":  readOnlyPointers,
		"total_access_count": totalAccess,
		"permissions_count":  len(pm.permissions),
	}
}

// Cleanup invalidates all pointers and clears management state
func (pm *PointerManager) Cleanup() {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	// Invalidate all pointers
	for _, ptr := range pm.pointers {
		ptr.valid.Store(false)
	}

	// Clear maps
	pm.pointers = make(map[uint32]*Pointer)
	pm.permissions = make(map[uint32]PointerPermission)
}

// getCurrentTimestamp returns current Unix timestamp (helper function)
func getCurrentTimestamp() int64 {
	return time.Now().Unix()
}

// String returns a string representation of the pointer
func (ptr *Pointer) String() string {
	return fmt.Sprintf("Pointer{addr: 0x%x, size: %d, align: %d, readonly: %t, valid: %t, tag: %s, access: %d}",
		ptr.address, ptr.size, ptr.alignment, ptr.readOnly, ptr.valid.Load(), ptr.tag, ptr.accessCount.Load())
}
