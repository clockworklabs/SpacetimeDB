package wasm

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
)

// AllocatorError represents allocator-related errors
type AllocatorError struct {
	Type      string
	Size      uint32
	Alignment uint32
	Message   string
	Context   map[string]interface{}
}

func (e *AllocatorError) Error() string {
	return fmt.Sprintf("allocator error [%s]: %s (size=%d, align=%d)", e.Type, e.Message, e.Size, e.Alignment)
}

// AllocationStrategy defines different allocation strategies
type AllocationStrategy int

const (
	// FirstFit allocates the first available block that fits
	FirstFit AllocationStrategy = iota
	// BestFit allocates the smallest available block that fits
	BestFit
	// WorstFit allocates the largest available block
	WorstFit
	// NextFit continues from the last allocation point
	NextFit
	// BuddySystem uses power-of-2 buddy allocation
	BuddySystem
	// PooledAllocation uses predefined size pools
	PooledAllocation
)

// AllocatorInterface defines the interface for custom allocators
type AllocatorInterface interface {
	// Allocate allocates memory of the specified size with alignment
	Allocate(ctx context.Context, size uint32, alignment uint32) (uint32, error)

	// Deallocate frees memory at the specified address
	Deallocate(ctx context.Context, address uint32) error

	// Reallocate changes the size of an existing allocation
	Reallocate(ctx context.Context, address uint32, newSize uint32) (uint32, error)

	// GetStats returns allocator statistics
	GetStats() map[string]interface{}

	// Cleanup releases all allocated memory
	Cleanup() error
}

// AllocationBlock represents a memory block in the allocator
type AllocationBlock struct {
	Address   uint32
	Size      uint32
	Free      bool
	Next      *AllocationBlock
	Prev      *AllocationBlock
	Tag       string
	Timestamp int64
}

// CustomAllocator implements a configurable memory allocator
type CustomAllocator struct {
	strategy   AllocationStrategy
	runtime    *Runtime
	freeBlocks *AllocationBlock // Head of free block list
	usedBlocks map[uint32]*AllocationBlock

	// Memory region management
	startAddress uint32
	endAddress   uint32
	totalSize    uint32

	// Statistics
	totalAllocated     atomic.Uint64
	totalDeallocated   atomic.Uint64
	currentAllocated   atomic.Uint64
	allocationCount    atomic.Uint64
	deallocationCount  atomic.Uint64
	fragmentationRatio atomic.Uint64 // Percentage * 100

	// Configuration
	minBlockSize     uint32
	maxBlockSize     uint32
	defaultAlignment uint32
	coalesceFree     bool // Whether to merge adjacent free blocks

	// Thread safety
	mu sync.RWMutex

	// Pool-based allocation (for PooledAllocation strategy)
	pools map[uint32]*AllocationPool

	// Buddy system (for BuddySystem strategy)
	buddyTree *BuddyTree

	// Next-fit state (for NextFit strategy)
	lastBlock *AllocationBlock
}

// AllocationPool represents a pool for fixed-size allocations
type AllocationPool struct {
	blockSize   uint32
	freeBlocks  []uint32 // Stack of free block addresses
	totalBlocks uint32
	usedBlocks  uint32
	baseAddress uint32
	mu          sync.Mutex
}

// BuddyTree implements a buddy system allocator
type BuddyTree struct {
	size     uint32 // Total size (must be power of 2)
	minSize  uint32 // Minimum allocation size
	levels   int    // Number of levels in the tree
	tree     []bool // Tree nodes (true = allocated, false = free)
	baseAddr uint32 // Base address of the managed region
	mu       sync.RWMutex
}

// AllocatorManager manages multiple allocators and allocation strategies
type AllocatorManager struct {
	allocators map[string]AllocatorInterface
	default_   AllocatorInterface
	runtime    *Runtime
	mu         sync.RWMutex

	// Global statistics
	totalAllocations    atomic.Uint64
	totalDeallocations  atomic.Uint64
	totalBytesAllocated atomic.Uint64
}

// NewCustomAllocator creates a new custom allocator
func NewCustomAllocator(runtime *Runtime, strategy AllocationStrategy, startAddr, size uint32) *CustomAllocator {
	allocator := &CustomAllocator{
		strategy:         strategy,
		runtime:          runtime,
		usedBlocks:       make(map[uint32]*AllocationBlock),
		startAddress:     startAddr,
		endAddress:       startAddr + size,
		totalSize:        size,
		minBlockSize:     16, // Minimum 16 bytes
		maxBlockSize:     size,
		defaultAlignment: 8, // 8-byte alignment by default
		coalesceFree:     true,
		pools:            make(map[uint32]*AllocationPool),
	}

	// Initialize with one large free block
	allocator.freeBlocks = &AllocationBlock{
		Address:   startAddr,
		Size:      size,
		Free:      true,
		Next:      nil,
		Prev:      nil,
		Tag:       "initial",
		Timestamp: getCurrentTimestamp(),
	}

	// Initialize strategy-specific data structures
	switch strategy {
	case BuddySystem:
		allocator.buddyTree = NewBuddyTree(startAddr, size, allocator.minBlockSize)
	case PooledAllocation:
		allocator.initializePools()
	case NextFit:
		allocator.lastBlock = allocator.freeBlocks
	}

	return allocator
}

// Allocate allocates memory using the configured strategy
func (ca *CustomAllocator) Allocate(ctx context.Context, size uint32, alignment uint32) (uint32, error) {
	if size == 0 {
		return 0, &AllocatorError{
			Type:    "invalid_size",
			Size:    size,
			Message: "cannot allocate zero bytes",
		}
	}

	if alignment == 0 {
		alignment = ca.defaultAlignment
	}

	// Ensure alignment is power of 2
	if alignment&(alignment-1) != 0 {
		return 0, &AllocatorError{
			Type:      "invalid_alignment",
			Size:      size,
			Alignment: alignment,
			Message:   "alignment must be power of 2",
		}
	}

	// Align size to minimum block size
	alignedSize := alignUp(size, ca.minBlockSize)

	var address uint32
	var err error

	// Use strategy-specific allocation
	switch ca.strategy {
	case FirstFit:
		address, err = ca.allocateFirstFit(alignedSize, alignment)
	case BestFit:
		address, err = ca.allocateBestFit(alignedSize, alignment)
	case WorstFit:
		address, err = ca.allocateWorstFit(alignedSize, alignment)
	case NextFit:
		address, err = ca.allocateNextFit(alignedSize, alignment)
	case BuddySystem:
		address, err = ca.allocateBuddy(alignedSize, alignment)
	case PooledAllocation:
		address, err = ca.allocatePooled(alignedSize, alignment)
	default:
		return 0, &AllocatorError{
			Type:    "unsupported_strategy",
			Size:    size,
			Message: fmt.Sprintf("unsupported allocation strategy: %d", ca.strategy),
		}
	}

	if err != nil {
		return 0, err
	}

	// Update statistics
	ca.totalAllocated.Add(uint64(alignedSize))
	ca.currentAllocated.Add(uint64(alignedSize))
	ca.allocationCount.Add(1)
	ca.updateFragmentation()

	return address, nil
}

// Deallocate frees memory at the specified address
func (ca *CustomAllocator) Deallocate(ctx context.Context, address uint32) error {
	ca.mu.Lock()
	defer ca.mu.Unlock()

	block, exists := ca.usedBlocks[address]
	if !exists {
		return &AllocatorError{
			Type:    "invalid_free",
			Message: fmt.Sprintf("attempt to free non-allocated address: 0x%x", address),
		}
	}

	// Strategy-specific deallocation
	switch ca.strategy {
	case BuddySystem:
		return ca.deallocateBuddy(address)
	case PooledAllocation:
		return ca.deallocatePooled(address, block.Size)
	default:
		return ca.deallocateStandard(address, block)
	}
}

// allocateFirstFit implements first-fit allocation strategy
func (ca *CustomAllocator) allocateFirstFit(size uint32, alignment uint32) (uint32, error) {
	ca.mu.Lock()
	defer ca.mu.Unlock()

	current := ca.freeBlocks
	for current != nil {
		if current.Free && current.Size >= size {
			// Check if we can align within this block
			alignedAddr := alignUp(current.Address, alignment)
			spaceNeeded := alignedAddr - current.Address + size

			if spaceNeeded <= current.Size {
				return ca.allocateFromBlock(current, alignedAddr, size)
			}
		}
		current = current.Next
	}

	return 0, &AllocatorError{
		Type:    "out_of_memory",
		Size:    size,
		Message: "no suitable free block found",
	}
}

// allocateBestFit implements best-fit allocation strategy
func (ca *CustomAllocator) allocateBestFit(size uint32, alignment uint32) (uint32, error) {
	ca.mu.Lock()
	defer ca.mu.Unlock()

	var bestBlock *AllocationBlock
	var bestAddr uint32
	bestWaste := uint32(0xFFFFFFFF) // Maximum waste

	current := ca.freeBlocks
	for current != nil {
		if current.Free && current.Size >= size {
			alignedAddr := alignUp(current.Address, alignment)
			spaceNeeded := alignedAddr - current.Address + size

			if spaceNeeded <= current.Size {
				waste := current.Size - spaceNeeded
				if waste < bestWaste {
					bestBlock = current
					bestAddr = alignedAddr
					bestWaste = waste
				}
			}
		}
		current = current.Next
	}

	if bestBlock == nil {
		return 0, &AllocatorError{
			Type:    "out_of_memory",
			Size:    size,
			Message: "no suitable free block found",
		}
	}

	return ca.allocateFromBlock(bestBlock, bestAddr, size)
}

// allocateWorstFit implements worst-fit allocation strategy
func (ca *CustomAllocator) allocateWorstFit(size uint32, alignment uint32) (uint32, error) {
	ca.mu.Lock()
	defer ca.mu.Unlock()

	var worstBlock *AllocationBlock
	var worstAddr uint32
	worstWaste := uint32(0) // Minimum waste

	current := ca.freeBlocks
	for current != nil {
		if current.Free && current.Size >= size {
			alignedAddr := alignUp(current.Address, alignment)
			spaceNeeded := alignedAddr - current.Address + size

			if spaceNeeded <= current.Size {
				waste := current.Size - spaceNeeded
				if waste > worstWaste {
					worstBlock = current
					worstAddr = alignedAddr
					worstWaste = waste
				}
			}
		}
		current = current.Next
	}

	if worstBlock == nil {
		return 0, &AllocatorError{
			Type:    "out_of_memory",
			Size:    size,
			Message: "no suitable free block found",
		}
	}

	return ca.allocateFromBlock(worstBlock, worstAddr, size)
}

// allocateNextFit implements next-fit allocation strategy
func (ca *CustomAllocator) allocateNextFit(size uint32, alignment uint32) (uint32, error) {
	ca.mu.Lock()
	defer ca.mu.Unlock()

	// Start from last allocation point
	current := ca.lastBlock
	if current == nil {
		current = ca.freeBlocks
	}

	// Search from last position to end
	for current != nil {
		if current.Free && current.Size >= size {
			alignedAddr := alignUp(current.Address, alignment)
			spaceNeeded := alignedAddr - current.Address + size

			if spaceNeeded <= current.Size {
				ca.lastBlock = current
				return ca.allocateFromBlock(current, alignedAddr, size)
			}
		}
		current = current.Next
	}

	// Search from beginning to last position
	current = ca.freeBlocks
	for current != ca.lastBlock && current != nil {
		if current.Free && current.Size >= size {
			alignedAddr := alignUp(current.Address, alignment)
			spaceNeeded := alignedAddr - current.Address + size

			if spaceNeeded <= current.Size {
				ca.lastBlock = current
				return ca.allocateFromBlock(current, alignedAddr, size)
			}
		}
		current = current.Next
	}

	return 0, &AllocatorError{
		Type:    "out_of_memory",
		Size:    size,
		Message: "no suitable free block found",
	}
}

// allocateFromBlock allocates memory from a specific block
func (ca *CustomAllocator) allocateFromBlock(block *AllocationBlock, address uint32, size uint32) (uint32, error) {
	// Create allocation record
	allocation := &AllocationBlock{
		Address:   address,
		Size:      size,
		Free:      false,
		Tag:       "allocated",
		Timestamp: getCurrentTimestamp(),
	}

	ca.usedBlocks[address] = allocation

	// Handle block splitting if necessary
	if block.Size > size {
		// Create remaining free block
		remainingSize := block.Size - (address - block.Address + size)
		if remainingSize > 0 {
			remaining := &AllocationBlock{
				Address:   address + size,
				Size:      remainingSize,
				Free:      true,
				Next:      block.Next,
				Prev:      block,
				Tag:       "remaining",
				Timestamp: getCurrentTimestamp(),
			}

			if block.Next != nil {
				block.Next.Prev = remaining
			}
			block.Next = remaining
		}

		// Adjust original block size
		block.Size = address - block.Address
		if block.Size == 0 {
			// Remove empty block
			ca.removeFromFreeList(block)
		}
	} else {
		// Use entire block
		ca.removeFromFreeList(block)
	}

	return address, nil
}

// removeFromFreeList removes a block from the free list
func (ca *CustomAllocator) removeFromFreeList(block *AllocationBlock) {
	if block.Prev != nil {
		block.Prev.Next = block.Next
	} else {
		ca.freeBlocks = block.Next
	}

	if block.Next != nil {
		block.Next.Prev = block.Prev
	}
}

// deallocateStandard implements standard deallocation with coalescing
func (ca *CustomAllocator) deallocateStandard(address uint32, block *AllocationBlock) error {
	// Update statistics
	ca.totalDeallocated.Add(uint64(block.Size))
	ca.currentAllocated.Add(^uint64(block.Size - 1)) // Subtract
	ca.deallocationCount.Add(1)

	// Remove from used blocks
	delete(ca.usedBlocks, address)

	// Create free block
	freeBlock := &AllocationBlock{
		Address:   address,
		Size:      block.Size,
		Free:      true,
		Tag:       "freed",
		Timestamp: getCurrentTimestamp(),
	}

	// Insert into free list (sorted by address)
	ca.insertIntoFreeList(freeBlock)

	// Coalesce adjacent free blocks if enabled
	if ca.coalesceFree {
		ca.coalesce(freeBlock)
	}

	ca.updateFragmentation()
	return nil
}

// insertIntoFreeList inserts a block into the free list in address order
func (ca *CustomAllocator) insertIntoFreeList(block *AllocationBlock) {
	if ca.freeBlocks == nil || block.Address < ca.freeBlocks.Address {
		// Insert at beginning
		block.Next = ca.freeBlocks
		if ca.freeBlocks != nil {
			ca.freeBlocks.Prev = block
		}
		ca.freeBlocks = block
		return
	}

	// Find insertion point
	current := ca.freeBlocks
	for current.Next != nil && current.Next.Address < block.Address {
		current = current.Next
	}

	// Insert after current
	block.Next = current.Next
	block.Prev = current
	if current.Next != nil {
		current.Next.Prev = block
	}
	current.Next = block
}

// coalesce merges adjacent free blocks
func (ca *CustomAllocator) coalesce(block *AllocationBlock) {
	// Coalesce with next block
	if block.Next != nil && block.Next.Free && block.Address+block.Size == block.Next.Address {
		next := block.Next
		block.Size += next.Size
		block.Next = next.Next
		if next.Next != nil {
			next.Next.Prev = block
		}
	}

	// Coalesce with previous block
	if block.Prev != nil && block.Prev.Free && block.Prev.Address+block.Prev.Size == block.Address {
		prev := block.Prev
		prev.Size += block.Size
		prev.Next = block.Next
		if block.Next != nil {
			block.Next.Prev = prev
		}
	}
}

// updateFragmentation calculates and updates fragmentation ratio
func (ca *CustomAllocator) updateFragmentation() {
	if ca.totalSize == 0 {
		return
	}

	freeBlocks := uint32(0)
	totalFreeSize := uint32(0)

	current := ca.freeBlocks
	for current != nil {
		if current.Free {
			freeBlocks++
			totalFreeSize += current.Size
		}
		current = current.Next
	}

	// Calculate fragmentation as (number of free blocks * 100) / total free size
	// Higher values indicate more fragmentation
	if totalFreeSize > 0 {
		fragmentation := (freeBlocks * 100) / (totalFreeSize / 1024) // Per KB
		ca.fragmentationRatio.Store(uint64(fragmentation))
	}
}

// GetStats returns allocator statistics
func (ca *CustomAllocator) GetStats() map[string]interface{} {
	return map[string]interface{}{
		"strategy":            ca.strategy,
		"total_allocated":     ca.totalAllocated.Load(),
		"total_deallocated":   ca.totalDeallocated.Load(),
		"current_allocated":   ca.currentAllocated.Load(),
		"allocation_count":    ca.allocationCount.Load(),
		"deallocation_count":  ca.deallocationCount.Load(),
		"fragmentation_ratio": ca.fragmentationRatio.Load(),
		"total_size":          ca.totalSize,
		"start_address":       fmt.Sprintf("0x%x", ca.startAddress),
		"end_address":         fmt.Sprintf("0x%x", ca.endAddress),
	}
}

// Cleanup releases all allocated memory
func (ca *CustomAllocator) Cleanup() error {
	ca.mu.Lock()
	defer ca.mu.Unlock()

	// Clear all data structures
	ca.usedBlocks = make(map[uint32]*AllocationBlock)
	ca.freeBlocks = &AllocationBlock{
		Address:   ca.startAddress,
		Size:      ca.totalSize,
		Free:      true,
		Tag:       "reset",
		Timestamp: getCurrentTimestamp(),
	}

	// Reset statistics
	ca.totalAllocated.Store(0)
	ca.totalDeallocated.Store(0)
	ca.currentAllocated.Store(0)
	ca.allocationCount.Store(0)
	ca.deallocationCount.Store(0)
	ca.fragmentationRatio.Store(0)

	return nil
}

// Reallocate changes the size of an existing allocation
func (ca *CustomAllocator) Reallocate(ctx context.Context, address uint32, newSize uint32) (uint32, error) {
	ca.mu.Lock()
	block, exists := ca.usedBlocks[address]
	ca.mu.Unlock()

	if !exists {
		return 0, &AllocatorError{
			Type:    "invalid_realloc",
			Size:    newSize,
			Message: fmt.Sprintf("attempt to reallocate non-allocated address: 0x%x", address),
		}
	}

	if newSize == 0 {
		// Realloc with size 0 is equivalent to free
		return 0, ca.Deallocate(ctx, address)
	}

	if newSize == block.Size {
		// No change needed
		return address, nil
	}

	// For simplicity, allocate new block and copy data
	newAddr, err := ca.Allocate(ctx, newSize, ca.defaultAlignment)
	if err != nil {
		return 0, err
	}

	// Copy existing data
	copySize := block.Size
	if newSize < copySize {
		copySize = newSize
	}

	// Read old data and write to new location
	if copySize > 0 {
		data, err := ca.runtime.ReadFromMemory(address, copySize)
		if err != nil {
			// Clean up new allocation on error
			ca.Deallocate(ctx, newAddr)
			return 0, err
		}

		err = ca.runtime.WriteToMemoryAt(newAddr, data)
		if err != nil {
			// Clean up new allocation on error
			ca.Deallocate(ctx, newAddr)
			return 0, err
		}
	}

	// Free old allocation
	ca.Deallocate(ctx, address)

	return newAddr, nil
}

// alignUp aligns a value up to the nearest multiple of alignment
func alignUp(value, alignment uint32) uint32 {
	return (value + alignment - 1) & ^(alignment - 1)
}

// initializePools creates predefined pools for pooled allocation
func (ca *CustomAllocator) initializePools() {
	poolSizes := []uint32{16, 32, 64, 128, 256, 512, 1024, 2048, 4096}

	currentAddr := ca.startAddress
	for _, size := range poolSizes {
		if currentAddr+size*100 <= ca.endAddress { // 100 blocks per pool
			pool := &AllocationPool{
				blockSize:   size,
				freeBlocks:  make([]uint32, 0, 100),
				totalBlocks: 100,
				usedBlocks:  0,
				baseAddress: currentAddr,
			}

			// Initialize free block list
			for i := uint32(0); i < 100; i++ {
				pool.freeBlocks = append(pool.freeBlocks, currentAddr+i*size)
			}

			ca.pools[size] = pool
			currentAddr += size * 100
		}
	}
}

// allocatePooled implements pooled allocation strategy
func (ca *CustomAllocator) allocatePooled(size uint32, alignment uint32) (uint32, error) {
	// Find appropriate pool
	var targetPool *AllocationPool
	for poolSize, pool := range ca.pools {
		if size <= poolSize {
			targetPool = pool
			break
		}
	}

	if targetPool == nil {
		// Fall back to first-fit for large allocations
		return ca.allocateFirstFit(size, alignment)
	}

	targetPool.mu.Lock()
	defer targetPool.mu.Unlock()

	if len(targetPool.freeBlocks) == 0 {
		return 0, &AllocatorError{
			Type:    "pool_exhausted",
			Size:    size,
			Message: fmt.Sprintf("no free blocks in pool (block size: %d)", targetPool.blockSize),
		}
	}

	// Get block from pool
	address := targetPool.freeBlocks[len(targetPool.freeBlocks)-1]
	targetPool.freeBlocks = targetPool.freeBlocks[:len(targetPool.freeBlocks)-1]
	targetPool.usedBlocks++

	// Record allocation
	allocation := &AllocationBlock{
		Address:   address,
		Size:      targetPool.blockSize,
		Free:      false,
		Tag:       "pooled",
		Timestamp: getCurrentTimestamp(),
	}

	ca.mu.Lock()
	ca.usedBlocks[address] = allocation
	ca.mu.Unlock()

	return address, nil
}

// deallocatePooled implements pooled deallocation
func (ca *CustomAllocator) deallocatePooled(address uint32, size uint32) error {
	// Find the pool this address belongs to
	var targetPool *AllocationPool
	for _, pool := range ca.pools {
		if address >= pool.baseAddress && address < pool.baseAddress+pool.blockSize*pool.totalBlocks {
			targetPool = pool
			break
		}
	}

	if targetPool == nil {
		return ca.deallocateStandard(address, ca.usedBlocks[address])
	}

	targetPool.mu.Lock()
	defer targetPool.mu.Unlock()

	// Return block to pool
	targetPool.freeBlocks = append(targetPool.freeBlocks, address)
	targetPool.usedBlocks--

	// Remove from used blocks
	delete(ca.usedBlocks, address)

	// Update statistics
	ca.totalDeallocated.Add(uint64(size))
	ca.currentAllocated.Add(^uint64(size - 1))
	ca.deallocationCount.Add(1)

	return nil
}

// Buddy system allocation methods (simplified implementation)
func NewBuddyTree(baseAddr, size, minSize uint32) *BuddyTree {
	// Size must be power of 2
	if size&(size-1) != 0 {
		// Round up to next power of 2
		size = nextPowerOf2(size)
	}

	levels := 0
	temp := size / minSize
	for temp > 1 {
		temp /= 2
		levels++
	}

	treeSize := (1 << (levels + 1)) - 1

	return &BuddyTree{
		size:     size,
		minSize:  minSize,
		levels:   levels,
		tree:     make([]bool, treeSize),
		baseAddr: baseAddr,
	}
}

func (ca *CustomAllocator) allocateBuddy(size uint32, alignment uint32) (uint32, error) {
	if ca.buddyTree == nil {
		return 0, &AllocatorError{
			Type:    "buddy_not_init",
			Size:    size,
			Message: "buddy system not initialized",
		}
	}

	// Round size up to power of 2
	buddySize := nextPowerOf2(size)
	if buddySize < ca.buddyTree.minSize {
		buddySize = ca.buddyTree.minSize
	}

	// Find and allocate block
	offset, err := ca.buddyTree.allocate(buddySize)
	if err != nil {
		return 0, err
	}

	address := ca.buddyTree.baseAddr + offset

	// Record allocation
	allocation := &AllocationBlock{
		Address:   address,
		Size:      buddySize,
		Free:      false,
		Tag:       "buddy",
		Timestamp: getCurrentTimestamp(),
	}

	ca.mu.Lock()
	ca.usedBlocks[address] = allocation
	ca.mu.Unlock()

	return address, nil
}

func (ca *CustomAllocator) deallocateBuddy(address uint32) error {
	ca.mu.Lock()
	block, exists := ca.usedBlocks[address]
	if !exists {
		ca.mu.Unlock()
		return &AllocatorError{
			Type:    "invalid_buddy_free",
			Message: fmt.Sprintf("attempt to free non-allocated buddy address: 0x%x", address),
		}
	}
	delete(ca.usedBlocks, address)
	ca.mu.Unlock()

	offset := address - ca.buddyTree.baseAddr
	ca.buddyTree.deallocate(offset, block.Size)

	// Update statistics
	ca.totalDeallocated.Add(uint64(block.Size))
	ca.currentAllocated.Add(^uint64(block.Size - 1))
	ca.deallocationCount.Add(1)

	return nil
}

// Simplified buddy tree operations
func (bt *BuddyTree) allocate(size uint32) (uint32, error) {
	bt.mu.Lock()
	defer bt.mu.Unlock()

	// This is a simplified implementation
	// In a full implementation, you would traverse the tree to find free buddies
	// For now, just linear search for demonstration

	return 0, &AllocatorError{
		Type:    "buddy_not_implemented",
		Size:    size,
		Message: "buddy allocation not fully implemented",
	}
}

func (bt *BuddyTree) deallocate(offset, size uint32) {
	bt.mu.Lock()
	defer bt.mu.Unlock()

	// Simplified deallocation
	// In a full implementation, you would mark buddies as free and merge
}

// nextPowerOf2 returns the next power of 2 greater than or equal to n
func nextPowerOf2(n uint32) uint32 {
	if n == 0 {
		return 1
	}
	n--
	n |= n >> 1
	n |= n >> 2
	n |= n >> 4
	n |= n >> 8
	n |= n >> 16
	return n + 1
}

// NewAllocatorManager creates a new allocator manager
func NewAllocatorManager(runtime *Runtime) *AllocatorManager {
	// Create a default first-fit allocator
	defaultAllocator := NewCustomAllocator(runtime, FirstFit, 0x10000, 1024*1024) // 1MB starting at 64KB

	manager := &AllocatorManager{
		allocators: make(map[string]AllocatorInterface),
		default_:   defaultAllocator,
		runtime:    runtime,
	}

	manager.allocators["default"] = defaultAllocator

	return manager
}

// RegisterAllocator registers a named allocator
func (am *AllocatorManager) RegisterAllocator(name string, allocator AllocatorInterface) {
	am.mu.Lock()
	defer am.mu.Unlock()
	am.allocators[name] = allocator
}

// GetAllocator returns a named allocator
func (am *AllocatorManager) GetAllocator(name string) AllocatorInterface {
	am.mu.RLock()
	defer am.mu.RUnlock()

	if allocator, exists := am.allocators[name]; exists {
		return allocator
	}
	return am.default_
}

// Allocate allocates using the default allocator
func (am *AllocatorManager) Allocate(ctx context.Context, size uint32, alignment uint32) (uint32, error) {
	address, err := am.default_.Allocate(ctx, size, alignment)
	if err == nil {
		am.totalAllocations.Add(1)
		am.totalBytesAllocated.Add(uint64(size))
	}
	return address, err
}

// Deallocate deallocates using the default allocator
func (am *AllocatorManager) Deallocate(ctx context.Context, address uint32) error {
	err := am.default_.Deallocate(ctx, address)
	if err == nil {
		am.totalDeallocations.Add(1)
	}
	return err
}

// GetGlobalStats returns global allocation statistics
func (am *AllocatorManager) GetGlobalStats() map[string]interface{} {
	am.mu.RLock()
	defer am.mu.RUnlock()

	allocatorStats := make(map[string]interface{})
	for name, allocator := range am.allocators {
		allocatorStats[name] = allocator.GetStats()
	}

	return map[string]interface{}{
		"total_allocations":     am.totalAllocations.Load(),
		"total_deallocations":   am.totalDeallocations.Load(),
		"total_bytes_allocated": am.totalBytesAllocated.Load(),
		"allocator_count":       len(am.allocators),
		"allocator_stats":       allocatorStats,
	}
}

// String returns a string representation of an allocation block
func (ab *AllocationBlock) String() string {
	return fmt.Sprintf("Block{addr: 0x%x, size: %d, free: %t, tag: %s}",
		ab.Address, ab.Size, ab.Free, ab.Tag)
}
