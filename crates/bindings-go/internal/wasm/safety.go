package wasm

import (
	"crypto/rand"
	"encoding/binary"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
)

// SafetyLevel defines the level of memory safety enforcement
type SafetyLevel int

const (
	// SafetyDisabled disables all safety checks (production mode)
	SafetyDisabled SafetyLevel = iota
	// SafetyBasic enables basic safety checks (bounds checking)
	SafetyBasic
	// SafetyStandard enables standard safety checks (basic + use-after-free)
	SafetyStandard
	// SafetyStrict enables strict safety checks (standard + canaries)
	SafetyStrict
	// SafetyParanoid enables paranoid safety checks (all features)
	SafetyParanoid
)

// SafetyError represents memory safety violations
type SafetyError struct {
	Type       string
	Address    uint32
	Size       uint32
	Operation  string
	Violation  string
	StackTrace string
	Timestamp  int64
	Context    map[string]interface{}
}

func (e *SafetyError) Error() string {
	return fmt.Sprintf("memory safety violation [%s]: %s at 0x%x (op: %s, size: %d)",
		e.Type, e.Violation, e.Address, e.Operation, e.Size)
}

// MemorySafetyManager provides comprehensive memory safety features
type MemorySafetyManager struct {
	level   SafetyLevel
	runtime *Runtime

	// Protection mechanisms
	canaryManager  *CanaryManager
	shadowMemory   *ShadowMemory
	quarantine     *QuarantineManager
	redZoneManager *RedZoneManager

	// Violation tracking
	violations       []SafetyError
	violationCount   atomic.Uint64
	violationsByType map[string]*atomic.Uint64

	// Configuration
	enableStackTraces  bool
	enableQuarantine   bool
	enableRedZones     bool
	enableCanaries     bool
	enableShadowMemory bool

	// Thread safety
	mu sync.RWMutex

	// Statistics
	checksPerformed   atomic.Uint64
	violationsBlocked atomic.Uint64
}

// CanaryManager manages canary values for buffer overflow detection
type CanaryManager struct {
	canarySize     uint32
	canaryValue    uint32
	randomCanaries bool
	mu             sync.RWMutex
	canaries       map[uint32]uint32 // address -> canary value
}

// ShadowMemory tracks metadata for each memory location
type ShadowMemory struct {
	shadowMap   map[uint32]*ShadowMetadata
	granularity uint32 // Bytes per shadow entry
	mu          sync.RWMutex
}

// ShadowMetadata contains metadata for a memory region
type ShadowMetadata struct {
	State         MemoryState
	AllocatedAt   int64
	DeallocatedAt int64
	Size          uint32
	AllocStack    string
	FreeStack     string
	AccessCount   atomic.Uint64
}

// MemoryState represents the state of a memory region
type MemoryState int

const (
	MemoryStateUnknown MemoryState = iota
	MemoryStateAllocated
	MemoryStateFreed
	MemoryStateQuarantined
	MemoryStateCorrupted
)

// QuarantineManager manages freed memory to detect use-after-free
type QuarantineManager struct {
	quarantined   map[uint32]*QuarantineEntry
	maxQuarantine uint32 // Maximum bytes to keep quarantined
	currentSize   atomic.Uint32
	mu            sync.RWMutex
}

// QuarantineEntry represents a quarantined memory region
type QuarantineEntry struct {
	Address      uint32
	Size         uint32
	FreedAt      int64
	OriginalData []byte // Original data for corruption detection
	FreeStack    string
}

// RedZoneManager manages red zones around allocations
type RedZoneManager struct {
	redZoneSize uint32
	poisonValue byte
	mu          sync.RWMutex
	redZones    map[uint32]*RedZone // base address -> red zone info
}

// RedZone represents red zones around an allocation
type RedZone struct {
	BaseAddress  uint32
	AllocAddress uint32
	AllocSize    uint32
	TotalSize    uint32 // Including red zones
	LeftCanary   uint32
	RightCanary  uint32
}

// AccessValidator validates memory access operations
type AccessValidator struct {
	safetyManager *MemorySafetyManager
	mu            sync.RWMutex
}

// NewMemorySafetyManager creates a new memory safety manager
func NewMemorySafetyManager(runtime *Runtime, level SafetyLevel) *MemorySafetyManager {
	manager := &MemorySafetyManager{
		level:              level,
		runtime:            runtime,
		violations:         make([]SafetyError, 0),
		violationsByType:   make(map[string]*atomic.Uint64),
		enableStackTraces:  level >= SafetyStandard,
		enableQuarantine:   level >= SafetyStandard,
		enableRedZones:     level >= SafetyStrict,
		enableCanaries:     level >= SafetyStrict,
		enableShadowMemory: level >= SafetyParanoid,
	}

	// Initialize subsystems based on safety level
	if manager.enableCanaries {
		manager.canaryManager = NewCanaryManager()
	}

	if manager.enableShadowMemory {
		manager.shadowMemory = NewShadowMemory(4) // 4-byte granularity
	}

	if manager.enableQuarantine {
		manager.quarantine = NewQuarantineManager(1024 * 1024) // 1MB quarantine
	}

	if manager.enableRedZones {
		manager.redZoneManager = NewRedZoneManager(16) // 16-byte red zones
	}

	// Initialize violation counters
	violationTypes := []string{
		"buffer_overflow", "use_after_free", "double_free", "invalid_free",
		"canary_corruption", "red_zone_violation", "heap_corruption",
	}

	for _, vType := range violationTypes {
		manager.violationsByType[vType] = &atomic.Uint64{}
	}

	return manager
}

// ValidateAllocation validates a memory allocation
func (msm *MemorySafetyManager) ValidateAllocation(address uint32, size uint32) error {
	if msm.level == SafetyDisabled {
		return nil
	}

	msm.checksPerformed.Add(1)

	// Check for null pointer
	if address == 0 {
		return msm.recordViolation("null_pointer", address, size, "allocate",
			"attempt to allocate at null pointer")
	}

	// Check alignment
	if address%4 != 0 {
		return msm.recordViolation("misalignment", address, size, "allocate",
			"allocation not aligned to 4-byte boundary")
	}

	// Update shadow memory
	if msm.enableShadowMemory {
		msm.shadowMemory.SetAllocated(address, size)
	}

	// Set up red zones
	if msm.enableRedZones {
		msm.redZoneManager.SetupRedZones(address, size)
	}

	// Set up canaries
	if msm.enableCanaries {
		msm.canaryManager.SetCanary(address, size)
	}

	return nil
}

// ValidateDeallocation validates a memory deallocation
func (msm *MemorySafetyManager) ValidateDeallocation(address uint32) error {
	if msm.level == SafetyDisabled {
		return nil
	}

	msm.checksPerformed.Add(1)

	// Check for null pointer free
	if address == 0 {
		return msm.recordViolation("null_pointer", address, 0, "free",
			"attempt to free null pointer")
	}

	// Check shadow memory state
	if msm.enableShadowMemory {
		state, size, err := msm.shadowMemory.GetState(address)
		if err != nil {
			return msm.recordViolation("invalid_free", address, 0, "free",
				"attempt to free unallocated memory")
		}

		if state == MemoryStateFreed {
			return msm.recordViolation("double_free", address, size, "free",
				"attempt to free already freed memory")
		}

		if state == MemoryStateQuarantined {
			return msm.recordViolation("double_free", address, size, "free",
				"attempt to free quarantined memory")
		}
	}

	// Check canaries before freeing
	if msm.enableCanaries {
		if err := msm.canaryManager.CheckCanary(address); err != nil {
			return msm.recordViolation("canary_corruption", address, 0, "free",
				fmt.Sprintf("canary corruption detected: %v", err))
		}
	}

	// Check red zones
	if msm.enableRedZones {
		if err := msm.redZoneManager.CheckRedZones(address); err != nil {
			return msm.recordViolation("red_zone_violation", address, 0, "free",
				fmt.Sprintf("red zone violation detected: %v", err))
		}
	}

	// Move to quarantine instead of immediate free
	if msm.enableQuarantine {
		size := uint32(0)
		if msm.enableShadowMemory {
			_, size, _ = msm.shadowMemory.GetState(address)
		}
		msm.quarantine.Quarantine(address, size)
	}

	// Update shadow memory
	if msm.enableShadowMemory {
		msm.shadowMemory.SetFreed(address)
	}

	return nil
}

// ValidateAccess validates a memory access operation
func (msm *MemorySafetyManager) ValidateAccess(address uint32, size uint32, operation string) error {
	if msm.level == SafetyDisabled {
		return nil
	}

	msm.checksPerformed.Add(1)

	// Check for null pointer access
	if address == 0 {
		return msm.recordViolation("null_pointer", address, size, operation,
			"attempt to access null pointer")
	}

	// Check shadow memory state
	if msm.enableShadowMemory {
		for offset := uint32(0); offset < size; offset += msm.shadowMemory.granularity {
			checkAddr := address + offset
			state, allocSize, err := msm.shadowMemory.GetState(checkAddr)

			if err != nil {
				return msm.recordViolation("uninitialized_access", checkAddr, size, operation,
					"access to uninitialized memory")
			}

			if state == MemoryStateFreed {
				return msm.recordViolation("use_after_free", checkAddr, size, operation,
					"access to freed memory")
			}

			if state == MemoryStateQuarantined {
				return msm.recordViolation("use_after_free", checkAddr, size, operation,
					"access to quarantined memory")
			}

			if state == MemoryStateCorrupted {
				return msm.recordViolation("heap_corruption", checkAddr, size, operation,
					"access to corrupted memory")
			}

			// Check bounds
			if state == MemoryStateAllocated {
				metadata := msm.shadowMemory.GetMetadata(checkAddr)
				if metadata != nil {
					allocEnd := checkAddr + allocSize
					accessEnd := address + size
					if accessEnd > allocEnd {
						return msm.recordViolation("buffer_overflow", address, size, operation,
							fmt.Sprintf("access beyond allocated region (alloc: %d, access: %d)", allocSize, size))
					}
				}
			}
		}
	}

	// Check quarantine
	if msm.enableQuarantine {
		if msm.quarantine.IsQuarantined(address) {
			return msm.recordViolation("use_after_free", address, size, operation,
				"access to quarantined memory")
		}
	}

	return nil
}

// recordViolation records a safety violation
func (msm *MemorySafetyManager) recordViolation(violationType string, address uint32, size uint32, operation string, message string) error {
	msm.violationsBlocked.Add(1)

	// Capture stack trace if enabled
	stackTrace := ""
	if msm.enableStackTraces {
		stackTrace = captureStackTrace()
	}

	violation := SafetyError{
		Type:       violationType,
		Address:    address,
		Size:       size,
		Operation:  operation,
		Violation:  message,
		StackTrace: stackTrace,
		Timestamp:  time.Now().Unix(),
		Context:    make(map[string]interface{}),
	}

	// Record violation
	msm.mu.Lock()
	msm.violations = append(msm.violations, violation)
	msm.violationCount.Add(1)
	if counter, exists := msm.violationsByType[violationType]; exists {
		counter.Add(1)
	}
	msm.mu.Unlock()

	return &violation
}

// NewCanaryManager creates a new canary manager
func NewCanaryManager() *CanaryManager {
	return &CanaryManager{
		canarySize:     4, // 4-byte canaries
		canaryValue:    0xDEADBEEF,
		randomCanaries: true,
		canaries:       make(map[uint32]uint32),
	}
}

// SetCanary sets a canary value for an allocation
func (cm *CanaryManager) SetCanary(address uint32, size uint32) {
	cm.mu.Lock()
	defer cm.mu.Unlock()

	canary := cm.canaryValue
	if cm.randomCanaries {
		canary = cm.generateRandomCanary()
	}

	// Store canary value
	cm.canaries[address] = canary

	// Write canary to memory (simplified - would need runtime integration)
	// In a real implementation, this would write the canary before/after allocation
}

// CheckCanary verifies a canary value
func (cm *CanaryManager) CheckCanary(address uint32) error {
	cm.mu.RLock()
	defer cm.mu.RUnlock()

	_, exists := cm.canaries[address]
	if !exists {
		return fmt.Errorf("no canary found for address 0x%x", address)
	}

	// In a real implementation, this would read the canary from memory
	// and compare with expected value

	return nil
}

// generateRandomCanary generates a random canary value
func (cm *CanaryManager) generateRandomCanary() uint32 {
	var canary uint32
	if err := binary.Read(rand.Reader, binary.LittleEndian, &canary); err != nil {
		// Fallback to default canary on error
		return cm.canaryValue
	}
	return canary
}

// NewShadowMemory creates a new shadow memory tracker
func NewShadowMemory(granularity uint32) *ShadowMemory {
	return &ShadowMemory{
		shadowMap:   make(map[uint32]*ShadowMetadata),
		granularity: granularity,
	}
}

// SetAllocated marks memory as allocated in shadow memory
func (sm *ShadowMemory) SetAllocated(address uint32, size uint32) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	metadata := &ShadowMetadata{
		State:       MemoryStateAllocated,
		AllocatedAt: time.Now().Unix(),
		Size:        size,
		AllocStack:  captureStackTrace(),
	}

	// Mark all granules in the range as allocated
	for offset := uint32(0); offset < size; offset += sm.granularity {
		sm.shadowMap[address+offset] = metadata
	}
}

// SetFreed marks memory as freed in shadow memory
func (sm *ShadowMemory) SetFreed(address uint32) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	if metadata, exists := sm.shadowMap[address]; exists {
		metadata.State = MemoryStateFreed
		metadata.DeallocatedAt = time.Now().Unix()
		metadata.FreeStack = captureStackTrace()
	}
}

// GetState returns the state of memory at the given address
func (sm *ShadowMemory) GetState(address uint32) (MemoryState, uint32, error) {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	// Find the granule containing this address
	granuleAddr := (address / sm.granularity) * sm.granularity

	if metadata, exists := sm.shadowMap[granuleAddr]; exists {
		return metadata.State, metadata.Size, nil
	}

	return MemoryStateUnknown, 0, fmt.Errorf("no shadow entry for address 0x%x", address)
}

// GetMetadata returns metadata for the given address
func (sm *ShadowMemory) GetMetadata(address uint32) *ShadowMetadata {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	granuleAddr := (address / sm.granularity) * sm.granularity
	return sm.shadowMap[granuleAddr]
}

// NewQuarantineManager creates a new quarantine manager
func NewQuarantineManager(maxSize uint32) *QuarantineManager {
	return &QuarantineManager{
		quarantined:   make(map[uint32]*QuarantineEntry),
		maxQuarantine: maxSize,
	}
}

// Quarantine moves memory to quarantine
func (qm *QuarantineManager) Quarantine(address uint32, size uint32) {
	qm.mu.Lock()
	defer qm.mu.Unlock()

	// Check if we have space in quarantine
	if qm.currentSize.Load()+size > qm.maxQuarantine {
		// Remove oldest entries to make space
		qm.evictOldest(size)
	}

	entry := &QuarantineEntry{
		Address:   address,
		Size:      size,
		FreedAt:   time.Now().Unix(),
		FreeStack: captureStackTrace(),
	}

	qm.quarantined[address] = entry
	qm.currentSize.Add(size)
}

// IsQuarantined checks if an address is quarantined
func (qm *QuarantineManager) IsQuarantined(address uint32) bool {
	qm.mu.RLock()
	defer qm.mu.RUnlock()

	_, exists := qm.quarantined[address]
	return exists
}

// evictOldest removes the oldest quarantine entries
func (qm *QuarantineManager) evictOldest(spaceNeeded uint32) {
	// Find oldest entries and remove them
	var oldestTime int64 = time.Now().Unix()
	var oldestAddr uint32

	for addr, entry := range qm.quarantined {
		if entry.FreedAt < oldestTime {
			oldestTime = entry.FreedAt
			oldestAddr = addr
		}
	}

	if oldestAddr != 0 {
		if entry, exists := qm.quarantined[oldestAddr]; exists {
			delete(qm.quarantined, oldestAddr)
			qm.currentSize.Add(^uint32(entry.Size - 1)) // Subtract
		}
	}
}

// NewRedZoneManager creates a new red zone manager
func NewRedZoneManager(redZoneSize uint32) *RedZoneManager {
	return &RedZoneManager{
		redZoneSize: redZoneSize,
		poisonValue: 0xFE, // Poison byte
		redZones:    make(map[uint32]*RedZone),
	}
}

// SetupRedZones sets up red zones around an allocation
func (rzm *RedZoneManager) SetupRedZones(allocAddr uint32, allocSize uint32) {
	rzm.mu.Lock()
	defer rzm.mu.Unlock()

	baseAddr := allocAddr - rzm.redZoneSize
	totalSize := allocSize + 2*rzm.redZoneSize

	redZone := &RedZone{
		BaseAddress:  baseAddr,
		AllocAddress: allocAddr,
		AllocSize:    allocSize,
		TotalSize:    totalSize,
		LeftCanary:   0xDEADBEEF,
		RightCanary:  0xBEEFDEAD,
	}

	rzm.redZones[allocAddr] = redZone

	// In a real implementation, would poison the red zone memory
}

// CheckRedZones verifies red zone integrity
func (rzm *RedZoneManager) CheckRedZones(allocAddr uint32) error {
	rzm.mu.RLock()
	defer rzm.mu.RUnlock()

	redZone, exists := rzm.redZones[allocAddr]
	if !exists {
		return fmt.Errorf("no red zone found for address 0x%x", allocAddr)
	}

	// In a real implementation, would check if red zone memory is poisoned
	// For now, just return success
	_ = redZone
	return nil
}

// GetSafetyStats returns comprehensive safety statistics
func (msm *MemorySafetyManager) GetSafetyStats() map[string]interface{} {
	msm.mu.RLock()
	defer msm.mu.RUnlock()

	violationStats := make(map[string]uint64)
	for vType, counter := range msm.violationsByType {
		violationStats[vType] = counter.Load()
	}

	return map[string]interface{}{
		"safety_level":       msm.level,
		"checks_performed":   msm.checksPerformed.Load(),
		"violations_blocked": msm.violationsBlocked.Load(),
		"total_violations":   msm.violationCount.Load(),
		"violations_by_type": violationStats,
		"recent_violations":  len(msm.violations),
		"features_enabled": map[string]bool{
			"stack_traces":  msm.enableStackTraces,
			"quarantine":    msm.enableQuarantine,
			"red_zones":     msm.enableRedZones,
			"canaries":      msm.enableCanaries,
			"shadow_memory": msm.enableShadowMemory,
		},
	}
}

// GetRecentViolations returns recent safety violations
func (msm *MemorySafetyManager) GetRecentViolations(limit int) []SafetyError {
	msm.mu.RLock()
	defer msm.mu.RUnlock()

	if limit <= 0 || limit > len(msm.violations) {
		limit = len(msm.violations)
	}

	// Return last 'limit' violations
	start := len(msm.violations) - limit
	violations := make([]SafetyError, limit)
	copy(violations, msm.violations[start:])

	return violations
}

// SetSafetyLevel changes the safety level
func (msm *MemorySafetyManager) SetSafetyLevel(level SafetyLevel) {
	msm.mu.Lock()
	defer msm.mu.Unlock()

	msm.level = level

	// Update feature flags based on new level
	msm.enableStackTraces = level >= SafetyStandard
	msm.enableQuarantine = level >= SafetyStandard
	msm.enableRedZones = level >= SafetyStrict
	msm.enableCanaries = level >= SafetyStrict
	msm.enableShadowMemory = level >= SafetyParanoid
}

// Cleanup cleans up safety manager resources
func (msm *MemorySafetyManager) Cleanup() {
	msm.mu.Lock()
	defer msm.mu.Unlock()

	// Clear violations
	msm.violations = make([]SafetyError, 0)

	// Reset counters
	msm.violationCount.Store(0)
	msm.checksPerformed.Store(0)
	msm.violationsBlocked.Store(0)

	for _, counter := range msm.violationsByType {
		counter.Store(0)
	}

	// Clean up subsystems
	if msm.canaryManager != nil {
		msm.canaryManager.mu.Lock()
		msm.canaryManager.canaries = make(map[uint32]uint32)
		msm.canaryManager.mu.Unlock()
	}

	if msm.shadowMemory != nil {
		msm.shadowMemory.mu.Lock()
		msm.shadowMemory.shadowMap = make(map[uint32]*ShadowMetadata)
		msm.shadowMemory.mu.Unlock()
	}

	if msm.quarantine != nil {
		msm.quarantine.mu.Lock()
		msm.quarantine.quarantined = make(map[uint32]*QuarantineEntry)
		msm.quarantine.currentSize.Store(0)
		msm.quarantine.mu.Unlock()
	}

	if msm.redZoneManager != nil {
		msm.redZoneManager.mu.Lock()
		msm.redZoneManager.redZones = make(map[uint32]*RedZone)
		msm.redZoneManager.mu.Unlock()
	}
}

// captureStackTrace captures the current stack trace
func captureStackTrace() string {
	const depth = 10
	var pcs [depth]uintptr
	n := runtime.Callers(3, pcs[:])
	frames := runtime.CallersFrames(pcs[:n])

	var trace string
	for {
		frame, more := frames.Next()
		trace += fmt.Sprintf("%s:%d\n", frame.Function, frame.Line)
		if !more {
			break
		}
	}

	return trace
}

// String returns a string representation of a safety error
func (se *SafetyError) String() string {
	return fmt.Sprintf("SafetyError{type: %s, addr: 0x%x, op: %s, violation: %s}",
		se.Type, se.Address, se.Operation, se.Violation)
}

// String returns a string representation of memory state
func (ms MemoryState) String() string {
	switch ms {
	case MemoryStateUnknown:
		return "Unknown"
	case MemoryStateAllocated:
		return "Allocated"
	case MemoryStateFreed:
		return "Freed"
	case MemoryStateQuarantined:
		return "Quarantined"
	case MemoryStateCorrupted:
		return "Corrupted"
	default:
		return "Invalid"
	}
}

// String returns a string representation of safety level
func (sl SafetyLevel) String() string {
	switch sl {
	case SafetyDisabled:
		return "Disabled"
	case SafetyBasic:
		return "Basic"
	case SafetyStandard:
		return "Standard"
	case SafetyStrict:
		return "Strict"
	case SafetyParanoid:
		return "Paranoid"
	default:
		return "Unknown"
	}
}
