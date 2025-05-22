package wasm

import (
	"fmt"
	"math"
	"sync"
	"sync/atomic"
)

// BoundsError represents bounds-related errors
type BoundsError struct {
	Type    string
	Address uint32
	Size    uint32
	Limit   uint32
	Message string
	Context map[string]interface{}
}

func (e *BoundsError) Error() string {
	return fmt.Sprintf("bounds error [%s]: %s (addr=0x%x, size=%d, limit=0x%x)",
		e.Type, e.Message, e.Address, e.Size, e.Limit)
}

// MemoryRegionManager manages memory regions and their bounds
type MemoryRegionManager struct {
	regions       map[uint32]*BoundedRegion
	sortedRegions []*BoundedRegion // Sorted by address for efficient lookups
	totalSize     atomic.Uint64
	regionCount   atomic.Uint32
	mu            sync.RWMutex

	// Configuration
	enableOverlapCheck  bool
	enableBoundsLogging bool
	maxRegions          uint32
	defaultAlignment    uint32
}

// BoundedRegion represents a memory region with strict bounds
type BoundedRegion struct {
	StartAddress uint32
	EndAddress   uint32 // Exclusive end
	Size         uint32
	Tag          string
	Permissions  RegionPermissions
	CreatedAt    int64
	LastAccessed int64
	AccessCount  atomic.Uint64

	// Bounds checking configuration
	StrictBounds  bool
	AllowGrowth   bool
	MaxGrowthSize uint32
	AlignmentReq  uint32
}

// RegionPermissions defines what operations are allowed in a region
type RegionPermissions struct {
	Read     bool
	Write    bool
	Execute  bool
	Grow     bool
	Metadata map[string]interface{}
}

// BoundsChecker provides comprehensive bounds checking functionality
type BoundsChecker struct {
	runtime       *Runtime
	regionManager *MemoryRegionManager

	// Overflow detection
	overflowDetector *OverflowDetector

	// Configuration
	enableStrictMode     bool
	enableOverflowCheck  bool
	enableUnderflowCheck bool
	logViolations        bool

	// Statistics
	checksPerformed atomic.Uint64
	violationsFound atomic.Uint64
	overflowsFound  atomic.Uint64
	underflowsFound atomic.Uint64
}

// OverflowDetector detects potential overflow conditions
type OverflowDetector struct {
	// Thresholds for different operations
	additionThreshold   uint32
	multiplicationLimit uint32
	sizeCheckThreshold  uint32

	// Statistics
	overflowsDetected atomic.Uint64
	nearMissCount     atomic.Uint64

	mu sync.RWMutex
}

// AccessInfo contains information about a memory access attempt
type AccessInfo struct {
	Address    uint32
	Size       uint32
	Operation  string // "read", "write", "execute"
	Caller     string
	Timestamp  int64
	Successful bool
	Error      error
}

// NewMemoryRegionManager creates a new memory region manager
func NewMemoryRegionManager() *MemoryRegionManager {
	return &MemoryRegionManager{
		regions:             make(map[uint32]*BoundedRegion),
		sortedRegions:       make([]*BoundedRegion, 0),
		enableOverlapCheck:  true,
		enableBoundsLogging: false,
		maxRegions:          10000,
		defaultAlignment:    4, // 4-byte alignment by default
	}
}

// AddRegion adds a new bounded region
func (mrm *MemoryRegionManager) AddRegion(startAddr, size uint32, tag string, permissions RegionPermissions) error {
	mrm.mu.Lock()
	defer mrm.mu.Unlock()

	if mrm.regionCount.Load() >= mrm.maxRegions {
		return &BoundsError{
			Type:    "region_limit",
			Address: startAddr,
			Size:    size,
			Message: fmt.Sprintf("maximum regions exceeded (%d)", mrm.maxRegions),
		}
	}

	endAddr := startAddr + size
	if endAddr < startAddr { // Overflow check
		return &BoundsError{
			Type:    "overflow",
			Address: startAddr,
			Size:    size,
			Message: "region size causes address overflow",
		}
	}

	// Check for overlaps if enabled
	if mrm.enableOverlapCheck {
		if err := mrm.checkOverlap(startAddr, endAddr); err != nil {
			return err
		}
	}

	region := &BoundedRegion{
		StartAddress:  startAddr,
		EndAddress:    endAddr,
		Size:          size,
		Tag:           tag,
		Permissions:   permissions,
		CreatedAt:     getCurrentTimestamp(),
		LastAccessed:  getCurrentTimestamp(),
		StrictBounds:  true,
		AllowGrowth:   false,
		MaxGrowthSize: 0,
		AlignmentReq:  mrm.defaultAlignment,
	}

	mrm.regions[startAddr] = region
	mrm.insertSorted(region)
	mrm.regionCount.Add(1)
	mrm.totalSize.Add(uint64(size))

	return nil
}

// RemoveRegion removes a bounded region
func (mrm *MemoryRegionManager) RemoveRegion(startAddr uint32) error {
	mrm.mu.Lock()
	defer mrm.mu.Unlock()

	region, exists := mrm.regions[startAddr]
	if !exists {
		return &BoundsError{
			Type:    "not_found",
			Address: startAddr,
			Message: "region not found",
		}
	}

	delete(mrm.regions, startAddr)
	mrm.removeSorted(region)
	mrm.regionCount.Add(^uint32(0))             // Subtract 1
	mrm.totalSize.Add(^uint64(region.Size - 1)) // Subtract size

	return nil
}

// FindRegion finds a region containing the given address
func (mrm *MemoryRegionManager) FindRegion(address uint32) *BoundedRegion {
	mrm.mu.RLock()
	defer mrm.mu.RUnlock()

	// Binary search through sorted regions
	left, right := 0, len(mrm.sortedRegions)-1

	for left <= right {
		mid := (left + right) / 2
		region := mrm.sortedRegions[mid]

		if address >= region.StartAddress && address < region.EndAddress {
			return region
		} else if address < region.StartAddress {
			right = mid - 1
		} else {
			left = mid + 1
		}
	}

	return nil
}

// insertSorted inserts a region into the sorted list
func (mrm *MemoryRegionManager) insertSorted(region *BoundedRegion) {
	// Find insertion point
	insertIndex := 0
	for i, r := range mrm.sortedRegions {
		if region.StartAddress < r.StartAddress {
			insertIndex = i
			break
		}
		insertIndex = i + 1
	}

	// Insert at the found position
	mrm.sortedRegions = append(mrm.sortedRegions, nil)
	copy(mrm.sortedRegions[insertIndex+1:], mrm.sortedRegions[insertIndex:])
	mrm.sortedRegions[insertIndex] = region
}

// removeSorted removes a region from the sorted list
func (mrm *MemoryRegionManager) removeSorted(region *BoundedRegion) {
	for i, r := range mrm.sortedRegions {
		if r == region {
			mrm.sortedRegions = append(mrm.sortedRegions[:i], mrm.sortedRegions[i+1:]...)
			break
		}
	}
}

// checkOverlap checks if a new region would overlap with existing ones
func (mrm *MemoryRegionManager) checkOverlap(startAddr, endAddr uint32) error {
	for _, region := range mrm.sortedRegions {
		// Check if ranges overlap
		if !(endAddr <= region.StartAddress || startAddr >= region.EndAddress) {
			return &BoundsError{
				Type:    "overlap",
				Address: startAddr,
				Size:    endAddr - startAddr,
				Limit:   region.EndAddress,
				Message: fmt.Sprintf("overlaps with region [0x%x-0x%x] tagged '%s'",
					region.StartAddress, region.EndAddress, region.Tag),
			}
		}
	}
	return nil
}

// NewBoundsChecker creates a new bounds checker
func NewBoundsChecker(runtime *Runtime) *BoundsChecker {
	return &BoundsChecker{
		runtime:              runtime,
		regionManager:        NewMemoryRegionManager(),
		overflowDetector:     NewOverflowDetector(),
		enableStrictMode:     true,
		enableOverflowCheck:  true,
		enableUnderflowCheck: true,
		logViolations:        false,
	}
}

// NewOverflowDetector creates a new overflow detector
func NewOverflowDetector() *OverflowDetector {
	return &OverflowDetector{
		additionThreshold:   math.MaxUint32 - 1024, // Leave some margin
		multiplicationLimit: 65536,                 // Reasonable limit for multiplications
		sizeCheckThreshold:  1024 * 1024 * 1024,    // 1GB
	}
}

// CheckBounds performs comprehensive bounds checking
func (bc *BoundsChecker) CheckBounds(address, size uint32, operation string) error {
	bc.checksPerformed.Add(1)

	// First check for arithmetic overflow
	if bc.enableOverflowCheck {
		if err := bc.checkArithmeticOverflow(address, size); err != nil {
			bc.overflowsFound.Add(1)
			bc.violationsFound.Add(1)
			return err
		}
	}

	// Check against WASM memory bounds
	if err := bc.checkWASMMemoryBounds(address, size); err != nil {
		bc.violationsFound.Add(1)
		return err
	}

	// Check against registered regions if in strict mode
	if bc.enableStrictMode {
		if err := bc.checkRegionBounds(address, size, operation); err != nil {
			bc.violationsFound.Add(1)
			return err
		}
	}

	// Check for underflow conditions
	if bc.enableUnderflowCheck {
		if err := bc.checkUnderflow(address, size); err != nil {
			bc.underflowsFound.Add(1)
			bc.violationsFound.Add(1)
			return err
		}
	}

	return nil
}

// checkArithmeticOverflow checks for arithmetic overflow conditions
func (bc *BoundsChecker) checkArithmeticOverflow(address, size uint32) error {
	// Check if address + size would overflow
	if address > math.MaxUint32-size {
		return &BoundsError{
			Type:    "arithmetic_overflow",
			Address: address,
			Size:    size,
			Limit:   math.MaxUint32,
			Message: "address + size exceeds 32-bit address space",
		}
	}

	// Check against overflow detector thresholds
	return bc.overflowDetector.CheckOverflow(address, size)
}

// checkWASMMemoryBounds checks bounds against WASM linear memory
func (bc *BoundsChecker) checkWASMMemoryBounds(address, size uint32) error {
	if bc.runtime.memory == nil {
		return &BoundsError{
			Type:    "memory_not_init",
			Address: address,
			Size:    size,
			Message: "WASM memory not initialized",
		}
	}

	memorySize := bc.runtime.memory.Size()
	endAddress := address + size

	if endAddress > memorySize {
		return &BoundsError{
			Type:    "memory_bounds",
			Address: address,
			Size:    size,
			Limit:   memorySize,
			Message: fmt.Sprintf("access beyond WASM memory (end: 0x%x, limit: 0x%x)", endAddress, memorySize),
		}
	}

	return nil
}

// checkRegionBounds checks bounds against registered memory regions
func (bc *BoundsChecker) checkRegionBounds(address, size uint32, operation string) error {
	region := bc.regionManager.FindRegion(address)
	if region == nil {
		return &BoundsError{
			Type:    "no_region",
			Address: address,
			Size:    size,
			Message: "access to unregistered memory region",
		}
	}

	endAddress := address + size
	if endAddress > region.EndAddress {
		return &BoundsError{
			Type:    "region_bounds",
			Address: address,
			Size:    size,
			Limit:   region.EndAddress,
			Message: fmt.Sprintf("access beyond region bounds (region: %s)", region.Tag),
		}
	}

	// Check permissions
	if err := bc.checkRegionPermissions(region, operation); err != nil {
		return err
	}

	// Update access tracking
	region.AccessCount.Add(1)
	region.LastAccessed = getCurrentTimestamp()

	return nil
}

// checkRegionPermissions validates operation permissions for a region
func (bc *BoundsChecker) checkRegionPermissions(region *BoundedRegion, operation string) error {
	switch operation {
	case "read":
		if !region.Permissions.Read {
			return &BoundsError{
				Type:    "permission",
				Address: region.StartAddress,
				Size:    region.Size,
				Message: "read permission denied for region: " + region.Tag,
			}
		}
	case "write":
		if !region.Permissions.Write {
			return &BoundsError{
				Type:    "permission",
				Address: region.StartAddress,
				Size:    region.Size,
				Message: "write permission denied for region: " + region.Tag,
			}
		}
	case "execute":
		if !region.Permissions.Execute {
			return &BoundsError{
				Type:    "permission",
				Address: region.StartAddress,
				Size:    region.Size,
				Message: "execute permission denied for region: " + region.Tag,
			}
		}
	}
	return nil
}

// checkUnderflow checks for underflow conditions
func (bc *BoundsChecker) checkUnderflow(address, size uint32) error {
	// Check if we're accessing before the start of memory
	if address < size && address != 0 {
		// This could indicate an underflow in pointer arithmetic
		return &BoundsError{
			Type:    "potential_underflow",
			Address: address,
			Size:    size,
			Message: "access near zero suggests potential underflow",
		}
	}

	return nil
}

// CheckOverflow checks for overflow in arithmetic operations
func (od *OverflowDetector) CheckOverflow(address, size uint32) error {
	od.mu.RLock()
	defer od.mu.RUnlock()

	// Check if adding size to address would exceed threshold
	if address > od.additionThreshold {
		od.overflowsDetected.Add(1)
		return &BoundsError{
			Type:    "addition_overflow",
			Address: address,
			Size:    size,
			Limit:   od.additionThreshold,
			Message: "address near overflow threshold",
		}
	}

	// Check if size itself is suspiciously large
	if size > od.sizeCheckThreshold {
		od.nearMissCount.Add(1)
		return &BoundsError{
			Type:    "large_size",
			Address: address,
			Size:    size,
			Limit:   od.sizeCheckThreshold,
			Message: "suspiciously large allocation size",
		}
	}

	return nil
}

// ValidateAlignment checks memory alignment requirements
func (bc *BoundsChecker) ValidateAlignment(address uint32, alignment uint32) error {
	if alignment == 0 || (alignment&(alignment-1)) != 0 {
		return &BoundsError{
			Type:    "invalid_alignment",
			Address: address,
			Message: fmt.Sprintf("alignment must be power of 2, got %d", alignment),
		}
	}

	if address%alignment != 0 {
		return &BoundsError{
			Type:    "misaligned_access",
			Address: address,
			Message: fmt.Sprintf("address 0x%x not aligned to %d bytes", address, alignment),
		}
	}

	return nil
}

// GetBoundsStats returns comprehensive bounds checking statistics
func (bc *BoundsChecker) GetBoundsStats() map[string]interface{} {
	regionStats := map[string]interface{}{
		"total_regions": bc.regionManager.regionCount.Load(),
		"total_size":    bc.regionManager.totalSize.Load(),
	}

	overflowStats := map[string]interface{}{
		"overflows_detected": bc.overflowDetector.overflowsDetected.Load(),
		"near_miss_count":    bc.overflowDetector.nearMissCount.Load(),
	}

	return map[string]interface{}{
		"checks_performed": bc.checksPerformed.Load(),
		"violations_found": bc.violationsFound.Load(),
		"overflows_found":  bc.overflowsFound.Load(),
		"underflows_found": bc.underflowsFound.Load(),
		"region_stats":     regionStats,
		"overflow_stats":   overflowStats,
		"strict_mode":      bc.enableStrictMode,
	}
}

// SetStrictMode enables or disables strict bounds checking
func (bc *BoundsChecker) SetStrictMode(enabled bool) {
	bc.enableStrictMode = enabled
}

// SetOverflowDetection enables or disables overflow detection
func (bc *BoundsChecker) SetOverflowDetection(enabled bool) {
	bc.enableOverflowCheck = enabled
}

// SetUnderflowDetection enables or disables underflow detection
func (bc *BoundsChecker) SetUnderflowDetection(enabled bool) {
	bc.enableUnderflowCheck = enabled
}

// RegisterRegion registers a new memory region for bounds checking
func (bc *BoundsChecker) RegisterRegion(startAddr, size uint32, tag string, read, write, execute bool) error {
	permissions := RegionPermissions{
		Read:     read,
		Write:    write,
		Execute:  execute,
		Grow:     false,
		Metadata: make(map[string]interface{}),
	}

	return bc.regionManager.AddRegion(startAddr, size, tag, permissions)
}

// UnregisterRegion removes a memory region from bounds checking
func (bc *BoundsChecker) UnregisterRegion(startAddr uint32) error {
	return bc.regionManager.RemoveRegion(startAddr)
}

// GetRegion returns information about a registered region
func (bc *BoundsChecker) GetRegion(address uint32) *BoundedRegion {
	return bc.regionManager.FindRegion(address)
}

// ListRegions returns all registered regions
func (bc *BoundsChecker) ListRegions() []*BoundedRegion {
	bc.regionManager.mu.RLock()
	defer bc.regionManager.mu.RUnlock()

	regions := make([]*BoundedRegion, len(bc.regionManager.sortedRegions))
	copy(regions, bc.regionManager.sortedRegions)
	return regions
}

// Cleanup clears all regions and resets statistics
func (bc *BoundsChecker) Cleanup() {
	bc.regionManager.mu.Lock()
	defer bc.regionManager.mu.Unlock()

	bc.regionManager.regions = make(map[uint32]*BoundedRegion)
	bc.regionManager.sortedRegions = make([]*BoundedRegion, 0)
	bc.regionManager.totalSize.Store(0)
	bc.regionManager.regionCount.Store(0)

	// Reset statistics
	bc.checksPerformed.Store(0)
	bc.violationsFound.Store(0)
	bc.overflowsFound.Store(0)
	bc.underflowsFound.Store(0)
	bc.overflowDetector.overflowsDetected.Store(0)
	bc.overflowDetector.nearMissCount.Store(0)
}

// String returns a string representation of a bounded region
func (br *BoundedRegion) String() string {
	return fmt.Sprintf("BoundedRegion{start: 0x%x, end: 0x%x, size: %d, tag: %s, accesses: %d}",
		br.StartAddress, br.EndAddress, br.Size, br.Tag, br.AccessCount.Load())
}
