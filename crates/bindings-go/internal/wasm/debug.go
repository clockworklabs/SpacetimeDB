package wasm

import (
	"context"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// DebugLevel defines the level of debug information
type DebugLevel int

const (
	// DebugOff disables all debug output
	DebugOff DebugLevel = iota
	// DebugError only logs errors
	DebugError
	// DebugWarn logs warnings and errors
	DebugWarn
	// DebugInfo logs informational messages
	DebugInfo
	// DebugVerbose logs detailed information
	DebugVerbose
	// DebugTrace logs everything including traces
	DebugTrace
)

// MemoryDebugger provides comprehensive memory debugging capabilities
type MemoryDebugger struct {
	runtime *Runtime
	level   DebugLevel
	output  io.Writer

	// Tracking
	allocTracker *AllocationTracker
	leakDetector *LeakDetector
	profiler     *MemoryProfiler
	analyzer     *MemoryAnalyzer

	// Configuration
	enableStackTraces   bool
	enableAllocationLog bool
	enableDumpOnError   bool
	maxLogEntries       int

	// Debugging state
	debugEnabled atomic.Bool
	logEntries   []DebugLogEntry
	mu           sync.RWMutex

	// Statistics
	debugCallsCount atomic.Uint64
	errorsLogged    atomic.Uint64
	warningsLogged  atomic.Uint64
}

// AllocationTracker tracks all memory allocations and deallocations
type AllocationTracker struct {
	allocations map[uint32]*AllocationInfo
	timeline    []AllocationEvent
	mu          sync.RWMutex

	// Statistics
	totalAllocations   atomic.Uint64
	totalDeallocations atomic.Uint64
	peakMemoryUsage    atomic.Uint64
	currentMemoryUsage atomic.Uint64
}

// AllocationInfo contains detailed information about an allocation
type AllocationInfo struct {
	Address    uint32
	Size       uint32
	Timestamp  int64
	StackTrace string
	Tag        string
	ThreadID   uint64
	CallSite   string
	Active     bool
}

// AllocationEvent represents an allocation or deallocation event
type AllocationEvent struct {
	Type       string // "alloc" or "free"
	Address    uint32
	Size       uint32
	Timestamp  int64
	StackTrace string
	Tag        string
}

// LeakDetector identifies potential memory leaks
type LeakDetector struct {
	suspiciousAllocations map[uint32]*AllocationInfo
	leakThreshold         time.Duration
	checkInterval         time.Duration
	mu                    sync.RWMutex

	// Leak detection state
	lastCheck      time.Time
	leaksDetected  atomic.Uint64
	falsePositives atomic.Uint64
}

// MemoryProfiler provides memory usage profiling
type MemoryProfiler struct {
	samples         []ProfileSample
	samplingEnabled atomic.Bool
	sampleInterval  time.Duration
	maxSamples      int
	mu              sync.RWMutex

	// Profiling statistics
	totalSamples atomic.Uint64
	averageUsage atomic.Uint64
	peakUsage    atomic.Uint64
}

// ProfileSample represents a memory usage sample
type ProfileSample struct {
	Timestamp     int64
	MemoryUsage   uint64
	Allocations   uint64
	Deallocations uint64
	ActiveBlocks  uint64
	Fragmentation float64
}

// MemoryAnalyzer performs advanced memory analysis
type MemoryAnalyzer struct {
	runtime *Runtime

	// Analysis results
	lastAnalysis    *AnalysisResult
	analysisHistory []AnalysisResult
	mu              sync.RWMutex
}

// AnalysisResult contains memory analysis results
type AnalysisResult struct {
	Timestamp          int64
	TotalMemory        uint64
	UsedMemory         uint64
	FreeMemory         uint64
	FragmentationRatio float64
	LargestFreeBlock   uint64
	SmallestFreeBlock  uint64
	AverageBlockSize   uint64
	Allocations        []AllocationInfo
	Issues             []MemoryIssue
}

// MemoryIssue represents a detected memory issue
type MemoryIssue struct {
	Type        string
	Severity    string
	Description string
	Address     uint32
	Size        uint32
	Suggestion  string
}

// DebugLogEntry represents a debug log entry
type DebugLogEntry struct {
	Level     DebugLevel
	Timestamp int64
	Message   string
	Context   map[string]interface{}
	Location  string
}

// DumpFormat defines memory dump formats
type DumpFormat int

const (
	DumpHex DumpFormat = iota
	DumpBinary
	DumpJSON
	DumpHtml
)

// NewMemoryDebugger creates a new memory debugger
func NewMemoryDebugger(runtime *Runtime, level DebugLevel) *MemoryDebugger {
	debugger := &MemoryDebugger{
		runtime:             runtime,
		level:               level,
		output:              os.Stdout,
		enableStackTraces:   level >= DebugInfo,
		enableAllocationLog: level >= DebugVerbose,
		enableDumpOnError:   level >= DebugWarn,
		maxLogEntries:       10000,
		logEntries:          make([]DebugLogEntry, 0),
	}

	debugger.debugEnabled.Store(level > DebugOff)

	// Initialize components
	debugger.allocTracker = NewAllocationTracker()
	debugger.leakDetector = NewLeakDetector(time.Minute * 5)    // 5-minute threshold
	debugger.profiler = NewMemoryProfiler(time.Second*10, 1000) // 10-second intervals, 1000 samples
	debugger.analyzer = NewMemoryAnalyzer(runtime)

	return debugger
}

// NewAllocationTracker creates a new allocation tracker
func NewAllocationTracker() *AllocationTracker {
	return &AllocationTracker{
		allocations: make(map[uint32]*AllocationInfo),
		timeline:    make([]AllocationEvent, 0),
	}
}

// TrackAllocation records a new allocation
func (at *AllocationTracker) TrackAllocation(address uint32, size uint32, tag string) {
	at.mu.Lock()
	defer at.mu.Unlock()

	now := time.Now().Unix()
	stackTrace := captureStackTrace()

	info := &AllocationInfo{
		Address:    address,
		Size:       size,
		Timestamp:  now,
		StackTrace: stackTrace,
		Tag:        tag,
		ThreadID:   getGoroutineID(),
		CallSite:   getCaller(),
		Active:     true,
	}

	at.allocations[address] = info

	// Add to timeline
	event := AllocationEvent{
		Type:       "alloc",
		Address:    address,
		Size:       size,
		Timestamp:  now,
		StackTrace: stackTrace,
		Tag:        tag,
	}
	at.timeline = append(at.timeline, event)

	// Update statistics
	at.totalAllocations.Add(1)
	newUsage := at.currentMemoryUsage.Add(uint64(size))

	// Update peak if necessary
	for {
		peak := at.peakMemoryUsage.Load()
		if newUsage <= peak || at.peakMemoryUsage.CompareAndSwap(peak, newUsage) {
			break
		}
	}
}

// TrackDeallocation records a deallocation
func (at *AllocationTracker) TrackDeallocation(address uint32) {
	at.mu.Lock()
	defer at.mu.Unlock()

	info, exists := at.allocations[address]
	if !exists {
		return // Already deallocated or invalid
	}

	info.Active = false
	now := time.Now().Unix()

	// Add to timeline
	event := AllocationEvent{
		Type:      "free",
		Address:   address,
		Size:      info.Size,
		Timestamp: now,
		Tag:       info.Tag,
	}
	at.timeline = append(at.timeline, event)

	// Update statistics
	at.totalDeallocations.Add(1)
	at.currentMemoryUsage.Add(^uint64(info.Size - 1)) // Subtract

	delete(at.allocations, address)
}

// GetActiveAllocations returns all active allocations
func (at *AllocationTracker) GetActiveAllocations() []AllocationInfo {
	at.mu.RLock()
	defer at.mu.RUnlock()

	active := make([]AllocationInfo, 0, len(at.allocations))
	for _, info := range at.allocations {
		if info.Active {
			active = append(active, *info)
		}
	}

	return active
}

// NewLeakDetector creates a new leak detector
func NewLeakDetector(threshold time.Duration) *LeakDetector {
	return &LeakDetector{
		suspiciousAllocations: make(map[uint32]*AllocationInfo),
		leakThreshold:         threshold,
		checkInterval:         time.Minute,
		lastCheck:             time.Now(),
	}
}

// CheckForLeaks identifies potential memory leaks
func (ld *LeakDetector) CheckForLeaks(allocations []AllocationInfo) []AllocationInfo {
	ld.mu.Lock()
	defer ld.mu.Unlock()

	now := time.Now()
	if now.Sub(ld.lastCheck) < ld.checkInterval {
		return nil // Too soon for next check
	}

	ld.lastCheck = now
	var leaks []AllocationInfo

	for _, alloc := range allocations {
		age := time.Duration(now.Unix()-alloc.Timestamp) * time.Second
		if age > ld.leakThreshold {
			leaks = append(leaks, alloc)
			ld.leaksDetected.Add(1)
		}
	}

	return leaks
}

// NewMemoryProfiler creates a new memory profiler
func NewMemoryProfiler(interval time.Duration, maxSamples int) *MemoryProfiler {
	return &MemoryProfiler{
		samples:        make([]ProfileSample, 0, maxSamples),
		sampleInterval: interval,
		maxSamples:     maxSamples,
	}
}

// StartProfiling begins memory profiling
func (mp *MemoryProfiler) StartProfiling(ctx context.Context, runtime *Runtime) {
	if !mp.samplingEnabled.CompareAndSwap(false, true) {
		return // Already running
	}

	ticker := time.NewTicker(mp.sampleInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			mp.samplingEnabled.Store(false)
			return
		case <-ticker.C:
			mp.takeSample(runtime)
		}
	}
}

// takeSample takes a memory usage sample
func (mp *MemoryProfiler) takeSample(runtime *Runtime) {
	mp.mu.Lock()
	defer mp.mu.Unlock()

	stats, err := runtime.GetMemoryStats()
	if err != nil {
		return
	}

	sample := ProfileSample{
		Timestamp:     time.Now().Unix(),
		MemoryUsage:   stats.Usage,
		Allocations:   stats.Allocs,
		Deallocations: stats.Frees,
		ActiveBlocks:  stats.Allocs - stats.Frees,
		Fragmentation: calculateFragmentation(stats),
	}

	// Add sample
	mp.samples = append(mp.samples, sample)

	// Remove oldest if over limit
	if len(mp.samples) > mp.maxSamples {
		mp.samples = mp.samples[1:]
	}

	mp.totalSamples.Add(1)
	mp.updateAverages()
}

// calculateFragmentation calculates memory fragmentation ratio
func calculateFragmentation(stats *MemoryStats) float64 {
	if stats.Size == 0 {
		return 0.0
	}
	return float64(uint64(stats.Size)-stats.Usage) / float64(stats.Size) * 100.0
}

// updateAverages updates average usage statistics
func (mp *MemoryProfiler) updateAverages() {
	if len(mp.samples) == 0 {
		return
	}

	var totalUsage uint64
	var peakUsage uint64

	for _, sample := range mp.samples {
		totalUsage += sample.MemoryUsage
		if sample.MemoryUsage > peakUsage {
			peakUsage = sample.MemoryUsage
		}
	}

	mp.averageUsage.Store(totalUsage / uint64(len(mp.samples)))
	mp.peakUsage.Store(peakUsage)
}

// NewMemoryAnalyzer creates a new memory analyzer
func NewMemoryAnalyzer(runtime *Runtime) *MemoryAnalyzer {
	return &MemoryAnalyzer{
		runtime:         runtime,
		analysisHistory: make([]AnalysisResult, 0),
	}
}

// AnalyzeMemory performs comprehensive memory analysis
func (ma *MemoryAnalyzer) AnalyzeMemory(allocations []AllocationInfo) *AnalysisResult {
	ma.mu.Lock()
	defer ma.mu.Unlock()

	stats, err := ma.runtime.GetMemoryStats()
	if err != nil {
		return nil
	}

	result := &AnalysisResult{
		Timestamp:          time.Now().Unix(),
		TotalMemory:        uint64(stats.Capacity),
		UsedMemory:         stats.Usage,
		FreeMemory:         uint64(stats.Capacity) - stats.Usage,
		FragmentationRatio: calculateFragmentation(stats),
		Allocations:        allocations,
		Issues:             make([]MemoryIssue, 0),
	}

	// Analyze allocation patterns
	ma.analyzeAllocationPatterns(result)

	// Detect issues
	ma.detectMemoryIssues(result)

	// Store result
	ma.lastAnalysis = result
	ma.analysisHistory = append(ma.analysisHistory, *result)

	// Limit history size
	if len(ma.analysisHistory) > 100 {
		ma.analysisHistory = ma.analysisHistory[1:]
	}

	return result
}

// analyzeAllocationPatterns analyzes memory allocation patterns
func (ma *MemoryAnalyzer) analyzeAllocationPatterns(result *AnalysisResult) {
	if len(result.Allocations) == 0 {
		return
	}

	// Calculate statistics
	var totalSize uint64
	var minSize, maxSize uint64 = ^uint64(0), 0

	for _, alloc := range result.Allocations {
		size := uint64(alloc.Size)
		totalSize += size
		if size < minSize {
			minSize = size
		}
		if size > maxSize {
			maxSize = size
		}
	}

	result.AverageBlockSize = totalSize / uint64(len(result.Allocations))
	result.LargestFreeBlock = maxSize
	result.SmallestFreeBlock = minSize
}

// detectMemoryIssues detects various memory issues
func (ma *MemoryAnalyzer) detectMemoryIssues(result *AnalysisResult) {
	// High fragmentation
	if result.FragmentationRatio > 50.0 {
		issue := MemoryIssue{
			Type:        "fragmentation",
			Severity:    "warning",
			Description: fmt.Sprintf("High memory fragmentation: %.1f%%", result.FragmentationRatio),
			Suggestion:  "Consider implementing memory compaction or using a different allocation strategy",
		}
		result.Issues = append(result.Issues, issue)
	}

	// Many small allocations
	smallAllocCount := 0
	for _, alloc := range result.Allocations {
		if alloc.Size < 64 { // Less than 64 bytes
			smallAllocCount++
		}
	}

	if smallAllocCount > len(result.Allocations)/2 {
		issue := MemoryIssue{
			Type:        "small_allocations",
			Severity:    "info",
			Description: fmt.Sprintf("Many small allocations detected: %d/%d", smallAllocCount, len(result.Allocations)),
			Suggestion:  "Consider using memory pools for small allocations",
		}
		result.Issues = append(result.Issues, issue)
	}

	// Low memory
	usageRatio := float64(result.UsedMemory) / float64(result.TotalMemory) * 100.0
	if usageRatio > 90.0 {
		issue := MemoryIssue{
			Type:        "low_memory",
			Severity:    "error",
			Description: fmt.Sprintf("Very high memory usage: %.1f%%", usageRatio),
			Suggestion:  "Consider freeing unused memory or increasing memory limits",
		}
		result.Issues = append(result.Issues, issue)
	}
}

// Log logs a debug message
func (md *MemoryDebugger) Log(level DebugLevel, message string, context map[string]interface{}) {
	if !md.debugEnabled.Load() || level > md.level {
		return
	}

	md.debugCallsCount.Add(1)

	entry := DebugLogEntry{
		Level:     level,
		Timestamp: time.Now().Unix(),
		Message:   message,
		Context:   context,
		Location:  getCaller(),
	}

	md.mu.Lock()
	defer md.mu.Unlock()

	// Add to log entries
	md.logEntries = append(md.logEntries, entry)

	// Limit log size
	if len(md.logEntries) > md.maxLogEntries {
		md.logEntries = md.logEntries[1:]
	}

	// Update counters
	switch level {
	case DebugError:
		md.errorsLogged.Add(1)
	case DebugWarn:
		md.warningsLogged.Add(1)
	}

	// Output to writer
	md.outputLogEntry(entry)
}

// outputLogEntry outputs a log entry to the configured writer
func (md *MemoryDebugger) outputLogEntry(entry DebugLogEntry) {
	timestamp := time.Unix(entry.Timestamp, 0).Format("2006-01-02 15:04:05")
	levelStr := entry.Level.String()

	fmt.Fprintf(md.output, "[%s] %s: %s at %s\n",
		timestamp, levelStr, entry.Message, entry.Location)

	if entry.Context != nil && len(entry.Context) > 0 {
		fmt.Fprintf(md.output, "  Context: %+v\n", entry.Context)
	}
}

// DumpMemory creates a memory dump in the specified format
func (md *MemoryDebugger) DumpMemory(format DumpFormat, address uint32, size uint32) ([]byte, error) {
	data, err := md.runtime.ReadFromMemory(address, size)
	if err != nil {
		return nil, fmt.Errorf("failed to read memory: %v", err)
	}

	switch format {
	case DumpHex:
		return md.formatHexDump(data, address), nil
	case DumpBinary:
		return data, nil
	case DumpJSON:
		return md.formatJSONDump(data, address)
	case DumpHtml:
		return md.formatHTMLDump(data, address), nil
	default:
		return nil, fmt.Errorf("unsupported dump format: %d", format)
	}
}

// formatHexDump formats memory as hex dump
func (md *MemoryDebugger) formatHexDump(data []byte, baseAddr uint32) []byte {
	var result strings.Builder

	result.WriteString(fmt.Sprintf("Memory dump at 0x%08x (%d bytes):\n", baseAddr, len(data)))
	result.WriteString("Address  : 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F | ASCII\n")
	result.WriteString("---------|------------------------------------------------|----------------\n")

	for i := 0; i < len(data); i += 16 {
		addr := baseAddr + uint32(i)
		result.WriteString(fmt.Sprintf("%08x:", addr))

		// Hex bytes
		for j := 0; j < 16; j++ {
			if i+j < len(data) {
				result.WriteString(fmt.Sprintf(" %02x", data[i+j]))
			} else {
				result.WriteString("   ")
			}
		}

		result.WriteString(" | ")

		// ASCII representation
		for j := 0; j < 16 && i+j < len(data); j++ {
			b := data[i+j]
			if b >= 32 && b <= 126 {
				result.WriteByte(b)
			} else {
				result.WriteByte('.')
			}
		}

		result.WriteString("\n")
	}

	return []byte(result.String())
}

// formatJSONDump formats memory as JSON
func (md *MemoryDebugger) formatJSONDump(data []byte, baseAddr uint32) ([]byte, error) {
	dump := map[string]interface{}{
		"base_address": fmt.Sprintf("0x%08x", baseAddr),
		"size":         len(data),
		"timestamp":    time.Now().Unix(),
		"hex_data":     hex.EncodeToString(data),
		"ascii_data":   string(data),
	}

	// Use a simple JSON formatting since we can't import encoding/json
	result := fmt.Sprintf(`{
  "base_address": "%s",
  "size": %d,
  "timestamp": %d,
  "hex_data": "%s",
  "ascii_data": "%s"
}`, dump["base_address"], dump["size"], dump["timestamp"], dump["hex_data"], dump["ascii_data"])

	return []byte(result), nil
}

// formatHTMLDump formats memory as HTML
func (md *MemoryDebugger) formatHTMLDump(data []byte, baseAddr uint32) []byte {
	var result strings.Builder

	result.WriteString(`<!DOCTYPE html>
<html>
<head>
    <title>Memory Dump</title>
    <style>
        body { font-family: monospace; margin: 20px; }
        .header { background-color: #f0f0f0; padding: 10px; }
        .hex-line { margin: 2px 0; }
        .address { color: #0066cc; }
        .hex { color: #000; }
        .ascii { color: #006600; }
    </style>
</head>
<body>
`)

	result.WriteString(fmt.Sprintf(`<div class="header">
        <h2>Memory Dump</h2>
        <p>Base Address: 0x%08x | Size: %d bytes | Generated: %s</p>
    </div>
    <pre>`, baseAddr, len(data), time.Now().Format("2006-01-02 15:04:05")))

	// Add hex dump content
	hexDump := md.formatHexDump(data, baseAddr)
	result.Write(hexDump)

	result.WriteString(`</pre>
</body>
</html>`)

	return []byte(result.String())
}

// GetDebugStats returns comprehensive debug statistics
func (md *MemoryDebugger) GetDebugStats() map[string]interface{} {
	md.mu.RLock()
	defer md.mu.RUnlock()

	return map[string]interface{}{
		"debug_level":      md.level.String(),
		"debug_enabled":    md.debugEnabled.Load(),
		"debug_calls":      md.debugCallsCount.Load(),
		"errors_logged":    md.errorsLogged.Load(),
		"warnings_logged":  md.warningsLogged.Load(),
		"log_entries":      len(md.logEntries),
		"allocation_stats": md.getAllocationStats(),
		"profiling_stats":  md.getProfilingStats(),
		"leak_stats":       md.getLeakStats(),
	}
}

// getAllocationStats returns allocation tracking statistics
func (md *MemoryDebugger) getAllocationStats() map[string]interface{} {
	return map[string]interface{}{
		"total_allocations":    md.allocTracker.totalAllocations.Load(),
		"total_deallocations":  md.allocTracker.totalDeallocations.Load(),
		"current_memory_usage": md.allocTracker.currentMemoryUsage.Load(),
		"peak_memory_usage":    md.allocTracker.peakMemoryUsage.Load(),
		"active_allocations":   len(md.allocTracker.allocations),
		"timeline_events":      len(md.allocTracker.timeline),
	}
}

// getProfilingStats returns profiling statistics
func (md *MemoryDebugger) getProfilingStats() map[string]interface{} {
	return map[string]interface{}{
		"sampling_enabled": md.profiler.samplingEnabled.Load(),
		"total_samples":    md.profiler.totalSamples.Load(),
		"average_usage":    md.profiler.averageUsage.Load(),
		"peak_usage":       md.profiler.peakUsage.Load(),
		"sample_count":     len(md.profiler.samples),
		"sample_interval":  md.profiler.sampleInterval.String(),
	}
}

// getLeakStats returns leak detection statistics
func (md *MemoryDebugger) getLeakStats() map[string]interface{} {
	return map[string]interface{}{
		"leaks_detected":    md.leakDetector.leaksDetected.Load(),
		"false_positives":   md.leakDetector.falsePositives.Load(),
		"leak_threshold":    md.leakDetector.leakThreshold.String(),
		"suspicious_allocs": len(md.leakDetector.suspiciousAllocations),
		"last_check":        md.leakDetector.lastCheck.Format("2006-01-02 15:04:05"),
	}
}

// ExportDebugData exports debug data for external analysis
func (md *MemoryDebugger) ExportDebugData() map[string]interface{} {
	md.mu.RLock()
	defer md.mu.RUnlock()

	// Export allocation timeline
	timeline := make([]map[string]interface{}, len(md.allocTracker.timeline))
	for i, event := range md.allocTracker.timeline {
		timeline[i] = map[string]interface{}{
			"type":      event.Type,
			"address":   fmt.Sprintf("0x%x", event.Address),
			"size":      event.Size,
			"timestamp": event.Timestamp,
			"tag":       event.Tag,
		}
	}

	// Export active allocations
	activeAllocs := md.allocTracker.GetActiveAllocations()
	allocations := make([]map[string]interface{}, len(activeAllocs))
	for i, alloc := range activeAllocs {
		allocations[i] = map[string]interface{}{
			"address":   fmt.Sprintf("0x%x", alloc.Address),
			"size":      alloc.Size,
			"timestamp": alloc.Timestamp,
			"tag":       alloc.Tag,
			"thread_id": alloc.ThreadID,
			"call_site": alloc.CallSite,
		}
	}

	// Export profile samples
	samples := make([]map[string]interface{}, len(md.profiler.samples))
	for i, sample := range md.profiler.samples {
		samples[i] = map[string]interface{}{
			"timestamp":     sample.Timestamp,
			"memory_usage":  sample.MemoryUsage,
			"allocations":   sample.Allocations,
			"deallocations": sample.Deallocations,
			"active_blocks": sample.ActiveBlocks,
			"fragmentation": sample.Fragmentation,
		}
	}

	return map[string]interface{}{
		"export_timestamp": time.Now().Unix(),
		"timeline":         timeline,
		"allocations":      allocations,
		"profile_samples":  samples,
		"statistics":       md.GetDebugStats(),
	}
}

// Helper functions

// getGoroutineID returns the current goroutine ID
func getGoroutineID() uint64 {
	// Simplified implementation - in real code, might use runtime inspection
	return 1
}

// getCaller returns information about the caller
func getCaller() string {
	pc, file, line, ok := runtime.Caller(2)
	if !ok {
		return "unknown"
	}

	fn := runtime.FuncForPC(pc)
	if fn == nil {
		return fmt.Sprintf("%s:%d", file, line)
	}

	return fmt.Sprintf("%s:%d", fn.Name(), line)
}

// String methods for debug types

// String returns string representation of debug level
func (dl DebugLevel) String() string {
	switch dl {
	case DebugOff:
		return "Off"
	case DebugError:
		return "Error"
	case DebugWarn:
		return "Warning"
	case DebugInfo:
		return "Info"
	case DebugVerbose:
		return "Verbose"
	case DebugTrace:
		return "Trace"
	default:
		return "Unknown"
	}
}

// String returns string representation of dump format
func (df DumpFormat) String() string {
	switch df {
	case DumpHex:
		return "Hex"
	case DumpBinary:
		return "Binary"
	case DumpJSON:
		return "JSON"
	case DumpHtml:
		return "HTML"
	default:
		return "Unknown"
	}
}

// String returns string representation of allocation info
func (ai *AllocationInfo) String() string {
	return fmt.Sprintf("Allocation{addr: 0x%x, size: %d, tag: %s, active: %t}",
		ai.Address, ai.Size, ai.Tag, ai.Active)
}

// String returns string representation of memory issue
func (mi *MemoryIssue) String() string {
	return fmt.Sprintf("Issue{type: %s, severity: %s, desc: %s}",
		mi.Type, mi.Severity, mi.Description)
}
