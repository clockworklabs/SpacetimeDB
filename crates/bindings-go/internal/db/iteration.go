package db

import (
	"context"
	"fmt"
	"io"
	"sync"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
)

// IteratorManager manages database iterators
type IteratorManager struct {
	mu              sync.RWMutex
	iterators       map[uint32]*IteratorMetadata
	activeIterators uint32
	nextIteratorID  uint32
	iteratorPool    sync.Pool
	iteratorStats   *IteratorStatistics
	maxActiveIters  uint32
	cleanupInterval time.Duration
	runtime         *runtime.Runtime
}

// IteratorMetadata contains iterator information
type IteratorMetadata struct {
	ID           uint32               `json:"id"`
	Type         IteratorType         `json:"type"`
	TableID      TableID              `json:"table_id"`
	IndexID      IndexID              `json:"index_id,omitempty"`
	Status       IteratorStatus       `json:"status"`
	CreatedAt    time.Time            `json:"created_at"`
	LastAccessAt time.Time            `json:"last_access_at"`
	Position     uint64               `json:"position"`
	TotalRows    uint64               `json:"total_rows"`
	Filter       *IteratorFilter      `json:"filter,omitempty"`
	Options      *IteratorOptions     `json:"options"`
	Performance  *IteratorPerformance `json:"performance"`
	Runtime      *runtime.Runtime     `json:"-"`
}

// IteratorType represents different types of iterators
type IteratorType int

const (
	IteratorTypeTableScan IteratorType = iota
	IteratorTypeIndexScan
	IteratorTypeFilteredScan
	IteratorTypeBatchScan
	IteratorTypeStreamScan
)

func (it IteratorType) String() string {
	switch it {
	case IteratorTypeTableScan:
		return "table_scan"
	case IteratorTypeIndexScan:
		return "index_scan"
	case IteratorTypeFilteredScan:
		return "filtered_scan"
	case IteratorTypeBatchScan:
		return "batch_scan"
	case IteratorTypeStreamScan:
		return "stream_scan"
	default:
		return "unknown"
	}
}

// IteratorStatus represents iterator status
type IteratorStatus int

const (
	IteratorStatusActive IteratorStatus = iota
	IteratorStatusExhausted
	IteratorStatusError
	IteratorStatusClosed
	IteratorStatusPaused
)

func (is IteratorStatus) String() string {
	switch is {
	case IteratorStatusActive:
		return "active"
	case IteratorStatusExhausted:
		return "exhausted"
	case IteratorStatusError:
		return "error"
	case IteratorStatusClosed:
		return "closed"
	case IteratorStatusPaused:
		return "paused"
	default:
		return "unknown"
	}
}

// IteratorFilter represents filtering options for iterators
type IteratorFilter struct {
	Predicate  string                 `json:"predicate"`
	Parameters map[string]interface{} `json:"parameters"`
	ColumnMask []bool                 `json:"column_mask"`
	Expression string                 `json:"expression"`
	Compiled   bool                   `json:"compiled"`
}

// IteratorOptions contains iterator configuration
type IteratorOptions struct {
	BatchSize    uint32                 `json:"batch_size"`
	Timeout      time.Duration          `json:"timeout"`
	Prefetch     bool                   `json:"prefetch"`
	CacheResults bool                   `json:"cache_results"`
	Direction    ScanDirection          `json:"direction"`
	Limit        uint64                 `json:"limit"`
	Offset       uint64                 `json:"offset"`
	Properties   map[string]interface{} `json:"properties"`
	ReadAhead    uint32                 `json:"read_ahead"`
	Compression  bool                   `json:"compression"`
	Async        bool                   `json:"async"`
}

// IteratorPerformance tracks iterator performance metrics
type IteratorPerformance struct {
	RowsRead       uint64        `json:"rows_read"`
	BytesRead      uint64        `json:"bytes_read"`
	ElapsedTime    time.Duration `json:"elapsed_time"`
	AverageRowTime time.Duration `json:"average_row_time"`
	CacheHits      uint64        `json:"cache_hits"`
	CacheMisses    uint64        `json:"cache_misses"`
	BufferHits     uint64        `json:"buffer_hits"`
	BufferMisses   uint64        `json:"buffer_misses"`
	SeekCount      uint64        `json:"seek_count"`
	FilteredRows   uint64        `json:"filtered_rows"`
	ThroughputMBps float64       `json:"throughput_mbps"`
}

// IteratorStatistics contains global iterator statistics
type IteratorStatistics struct {
	TotalCreated     uint64            `json:"total_created"`
	CurrentActive    uint32            `json:"current_active"`
	MaxConcurrent    uint32            `json:"max_concurrent"`
	TotalExhausted   uint64            `json:"total_exhausted"`
	TotalErrors      uint64            `json:"total_errors"`
	AverageLifetime  time.Duration     `json:"average_lifetime"`
	TypeDistribution map[string]uint64 `json:"type_distribution"`
	PerformanceStats *GlobalIterPerf   `json:"performance_stats"`
}

// GlobalIterPerf contains global iterator performance statistics
type GlobalIterPerf struct {
	TotalRowsRead     uint64    `json:"total_rows_read"`
	TotalBytesRead    uint64    `json:"total_bytes_read"`
	AverageThroughput float64   `json:"average_throughput"`
	PeakThroughput    float64   `json:"peak_throughput"`
	LastResetTime     time.Time `json:"last_reset_time"`
}

// EnhancedRowIter extends the basic RowIter with advanced features
type EnhancedRowIter struct {
	*RowIter
	metadata    *IteratorMetadata
	manager     *IteratorManager
	buffer      [][]byte
	bufferIndex int
	prefetched  bool
	ctx         context.Context
	cancel      context.CancelFunc
	errorChan   chan error
}

// BatchIterator provides batch iteration capabilities
type BatchIterator struct {
	iterator   *EnhancedRowIter
	batchSize  uint32
	buffer     [][]byte
	bufferSize int
	position   int
}

// StreamIterator provides streaming iteration capabilities
type StreamIterator struct {
	iterator   *EnhancedRowIter
	rowChan    chan []byte
	errorChan  chan error
	ctx        context.Context
	cancel     context.CancelFunc
	bufferSize int
}

// NewIteratorManager creates a new iterator manager
func NewIteratorManager(runtime *runtime.Runtime) *IteratorManager {
	manager := &IteratorManager{
		iterators:       make(map[uint32]*IteratorMetadata),
		nextIteratorID:  1,
		maxActiveIters:  1000,
		cleanupInterval: 5 * time.Minute,
		runtime:         runtime,
		iteratorStats: &IteratorStatistics{
			TypeDistribution: make(map[string]uint64),
			PerformanceStats: &GlobalIterPerf{
				LastResetTime: time.Now(),
			},
		},
	}

	// Initialize iterator pool
	manager.iteratorPool = sync.Pool{
		New: func() interface{} {
			return &EnhancedRowIter{
				buffer:    make([][]byte, 0, 100),
				errorChan: make(chan error, 1),
			}
		},
	}

	// Start cleanup routine
	go manager.cleanupRoutine()

	return manager
}

// CreateTableIterator creates an iterator for table scanning
func (im *IteratorManager) CreateTableIterator(tableID TableID, options *IteratorOptions) (*EnhancedRowIter, error) {
	im.mu.Lock()
	defer im.mu.Unlock()

	// Check limits
	if im.activeIterators >= im.maxActiveIters {
		return nil, fmt.Errorf("maximum active iterators exceeded")
	}

	// Set default options
	if options == nil {
		options = &IteratorOptions{
			BatchSize:  100,
			Timeout:    30 * time.Second,
			Prefetch:   true,
			Direction:  ScanDirectionForward,
			Properties: make(map[string]interface{}),
		}
	}

	// Generate iterator ID
	iteratorID := im.nextIteratorID
	im.nextIteratorID++

	// Create metadata
	metadata := &IteratorMetadata{
		ID:           iteratorID,
		Type:         IteratorTypeTableScan,
		TableID:      tableID,
		Status:       IteratorStatusActive,
		CreatedAt:    time.Now(),
		LastAccessAt: time.Now(),
		Position:     0,
		Options:      options,
		Performance:  &IteratorPerformance{},
		Runtime:      im.runtime,
	}

	// Create context for cancellation
	ctx, cancel := context.WithTimeout(context.Background(), options.Timeout)

	// Get iterator from pool
	enhancedIter := im.iteratorPool.Get().(*EnhancedRowIter)
	enhancedIter.metadata = metadata
	enhancedIter.manager = im
	enhancedIter.ctx = ctx
	enhancedIter.cancel = cancel
	enhancedIter.bufferIndex = 0
	enhancedIter.prefetched = false

	// Create underlying RowIter
	enhancedIter.RowIter = &RowIter{
		data:    [][]byte{},
		idx:     0,
		IterID:  iteratorID,
		Runtime: im.runtime,
	}

	// Store metadata
	im.iterators[iteratorID] = metadata
	im.activeIterators++

	// Update statistics
	im.iteratorStats.TotalCreated++
	im.iteratorStats.CurrentActive = im.activeIterators
	if im.activeIterators > im.iteratorStats.MaxConcurrent {
		im.iteratorStats.MaxConcurrent = im.activeIterators
	}
	im.iteratorStats.TypeDistribution[metadata.Type.String()]++

	// Start prefetching if enabled
	if options.Prefetch {
		go im.prefetchData(enhancedIter)
	}

	return enhancedIter, nil
}

// CreateIndexIterator creates an iterator for index scanning
func (im *IteratorManager) CreateIndexIterator(indexID IndexID, scanRange *IndexScanRange, options *IteratorOptions) (*EnhancedRowIter, error) {
	im.mu.Lock()
	defer im.mu.Unlock()

	// Check limits
	if im.activeIterators >= im.maxActiveIters {
		return nil, fmt.Errorf("maximum active iterators exceeded")
	}

	// Set default options
	if options == nil {
		options = &IteratorOptions{
			BatchSize:  100,
			Timeout:    30 * time.Second,
			Prefetch:   true,
			Direction:  scanRange.Direction,
			Properties: make(map[string]interface{}),
		}
	}

	// Generate iterator ID
	iteratorID := im.nextIteratorID
	im.nextIteratorID++

	// Create metadata
	metadata := &IteratorMetadata{
		ID:           iteratorID,
		Type:         IteratorTypeIndexScan,
		IndexID:      indexID,
		Status:       IteratorStatusActive,
		CreatedAt:    time.Now(),
		LastAccessAt: time.Now(),
		Position:     0,
		Options:      options,
		Performance:  &IteratorPerformance{},
		Runtime:      im.runtime,
	}

	// Create context for cancellation
	ctx, cancel := context.WithTimeout(context.Background(), options.Timeout)

	// Get iterator from pool
	enhancedIter := im.iteratorPool.Get().(*EnhancedRowIter)
	enhancedIter.metadata = metadata
	enhancedIter.manager = im
	enhancedIter.ctx = ctx
	enhancedIter.cancel = cancel
	enhancedIter.bufferIndex = 0
	enhancedIter.prefetched = false

	// Create underlying RowIter
	enhancedIter.RowIter = &RowIter{
		data:    [][]byte{},
		idx:     0,
		IterID:  iteratorID,
		Runtime: im.runtime,
	}

	// Store metadata
	im.iterators[iteratorID] = metadata
	im.activeIterators++

	// Update statistics
	im.updateStatistics(metadata.Type, true)

	return enhancedIter, nil
}

// CreateBatchIterator creates a batch iterator
func (im *IteratorManager) CreateBatchIterator(baseIter *EnhancedRowIter, batchSize uint32) *BatchIterator {
	return &BatchIterator{
		iterator:  baseIter,
		batchSize: batchSize,
		buffer:    make([][]byte, 0, batchSize),
		position:  0,
	}
}

// CreateStreamIterator creates a streaming iterator
func (im *IteratorManager) CreateStreamIterator(baseIter *EnhancedRowIter, bufferSize int) *StreamIterator {
	ctx, cancel := context.WithCancel(context.Background())

	streamIter := &StreamIterator{
		iterator:   baseIter,
		rowChan:    make(chan []byte, bufferSize),
		errorChan:  make(chan error, 1),
		ctx:        ctx,
		cancel:     cancel,
		bufferSize: bufferSize,
	}

	// Start streaming goroutine
	go streamIter.stream()

	return streamIter
}

// Read reads the next row with enhanced features
func (ei *EnhancedRowIter) Read() ([]byte, error) {
	ei.metadata.LastAccessAt = time.Now()
	startTime := time.Now()

	// Check context cancellation
	select {
	case <-ei.ctx.Done():
		return nil, ei.ctx.Err()
	default:
	}

	// Check for errors
	select {
	case err := <-ei.errorChan:
		return nil, err
	default:
	}

	// Try to read from buffer first
	if ei.bufferIndex < len(ei.buffer) {
		row := ei.buffer[ei.bufferIndex]
		ei.bufferIndex++
		ei.updatePerformanceMetrics(len(row), time.Since(startTime))
		return row, nil
	}

	// Read from underlying iterator
	row, err := ei.RowIter.Read()
	if err != nil {
		if err.Error() == "iterator exhausted" {
			ei.metadata.Status = IteratorStatusExhausted
			ei.manager.updateStatistics(ei.metadata.Type, false)
		} else {
			ei.metadata.Status = IteratorStatusError
		}
		return nil, err
	}

	ei.updatePerformanceMetrics(len(row), time.Since(startTime))
	return row, nil
}

// Close closes the enhanced iterator
func (ei *EnhancedRowIter) Close() error {
	ei.manager.mu.Lock()
	defer ei.manager.mu.Unlock()

	// Cancel context
	if ei.cancel != nil {
		ei.cancel()
	}

	// Update status
	ei.metadata.Status = IteratorStatusClosed

	// Update active count
	ei.manager.activeIterators--
	ei.manager.iteratorStats.CurrentActive = ei.manager.activeIterators

	// Return to pool
	ei.buffer = ei.buffer[:0]
	ei.bufferIndex = 0
	ei.prefetched = false
	ei.manager.iteratorPool.Put(ei)

	// Remove from active iterators
	delete(ei.manager.iterators, ei.metadata.ID)

	return ei.RowIter.Close()
}

// IsExhausted checks if the iterator is exhausted
func (ei *EnhancedRowIter) IsExhausted() bool {
	return ei.metadata.Status == IteratorStatusExhausted || ei.RowIter.IsExhausted()
}

// GetMetadata returns iterator metadata
func (ei *EnhancedRowIter) GetMetadata() *IteratorMetadata {
	return ei.metadata
}

// ReadBatch reads the next batch of rows
func (bi *BatchIterator) ReadBatch() ([][]byte, error) {
	bi.buffer = bi.buffer[:0]

	for i := uint32(0); i < bi.batchSize; i++ {
		row, err := bi.iterator.Read()
		if err != nil {
			if err.Error() == "iterator exhausted" {
				break
			}
			return nil, err
		}
		bi.buffer = append(bi.buffer, row)
	}

	if len(bi.buffer) == 0 {
		return nil, io.EOF
	}

	return bi.buffer, nil
}

// HasMoreBatches checks if more batches are available
func (bi *BatchIterator) HasMoreBatches() bool {
	return !bi.iterator.IsExhausted()
}

// Close closes the batch iterator
func (bi *BatchIterator) Close() error {
	return bi.iterator.Close()
}

// ReadRow reads a row from the stream
func (si *StreamIterator) ReadRow() ([]byte, error) {
	select {
	case row := <-si.rowChan:
		return row, nil
	case err := <-si.errorChan:
		return nil, err
	case <-si.ctx.Done():
		return nil, si.ctx.Err()
	}
}

// stream continuously streams rows in a goroutine
func (si *StreamIterator) stream() {
	defer close(si.rowChan)
	defer close(si.errorChan)

	for {
		select {
		case <-si.ctx.Done():
			return
		default:
			row, err := si.iterator.Read()
			if err != nil {
				if err.Error() == "iterator exhausted" {
					return
				}
				select {
				case si.errorChan <- err:
				case <-si.ctx.Done():
				}
				return
			}

			select {
			case si.rowChan <- row:
			case <-si.ctx.Done():
				return
			}
		}
	}
}

// Close closes the stream iterator
func (si *StreamIterator) Close() error {
	si.cancel()
	return si.iterator.Close()
}

// prefetchData prefetches data in the background
func (im *IteratorManager) prefetchData(iter *EnhancedRowIter) {
	batchSize := int(iter.metadata.Options.BatchSize)
	if batchSize <= 0 {
		batchSize = 100
	}

	// Prefetch a batch of rows
	for i := 0; i < batchSize && !iter.IsExhausted(); i++ {
		select {
		case <-iter.ctx.Done():
			return
		default:
			row, err := iter.RowIter.Read()
			if err != nil {
				if err.Error() == "iterator exhausted" {
					break
				}
				select {
				case iter.errorChan <- err:
				case <-iter.ctx.Done():
				}
				return
			}

			iter.buffer = append(iter.buffer, row)
		}
	}

	iter.prefetched = true
}

// updatePerformanceMetrics updates iterator performance metrics
func (ei *EnhancedRowIter) updatePerformanceMetrics(bytesRead int, duration time.Duration) {
	perf := ei.metadata.Performance
	perf.RowsRead++
	perf.BytesRead += uint64(bytesRead)
	perf.ElapsedTime += duration

	if perf.RowsRead > 0 {
		perf.AverageRowTime = time.Duration(int64(perf.ElapsedTime) / int64(perf.RowsRead))
	}

	// Calculate throughput
	if perf.ElapsedTime > 0 {
		mbps := float64(perf.BytesRead) / (1024.0 * 1024.0) / perf.ElapsedTime.Seconds()
		perf.ThroughputMBps = mbps
	}

	ei.metadata.Position = perf.RowsRead
}

// updateStatistics updates global iterator statistics
func (im *IteratorManager) updateStatistics(iterType IteratorType, created bool) {
	if created {
		im.iteratorStats.TotalCreated++
		im.iteratorStats.TypeDistribution[iterType.String()]++
	} else {
		im.iteratorStats.TotalExhausted++
	}
}

// GetIteratorStatistics returns global iterator statistics
func (im *IteratorManager) GetIteratorStatistics() *IteratorStatistics {
	im.mu.RLock()
	defer im.mu.RUnlock()

	// Create a copy to avoid concurrent modification
	stats := *im.iteratorStats
	stats.CurrentActive = im.activeIterators

	return &stats
}

// GetActiveIterators returns all active iterators
func (im *IteratorManager) GetActiveIterators() []*IteratorMetadata {
	im.mu.RLock()
	defer im.mu.RUnlock()

	iterators := make([]*IteratorMetadata, 0, len(im.iterators))
	for _, metadata := range im.iterators {
		iterators = append(iterators, metadata)
	}

	return iterators
}

// CloseAllIterators closes all active iterators
func (im *IteratorManager) CloseAllIterators() error {
	im.mu.Lock()
	defer im.mu.Unlock()

	var lastErr error
	for id, metadata := range im.iterators {
		metadata.Status = IteratorStatusClosed
		delete(im.iterators, id)
		im.activeIterators--
	}

	im.iteratorStats.CurrentActive = 0
	return lastErr
}

// cleanupRoutine periodically cleans up expired iterators
func (im *IteratorManager) cleanupRoutine() {
	ticker := time.NewTicker(im.cleanupInterval)
	defer ticker.Stop()

	for range ticker.C {
		im.cleanupExpiredIterators()
	}
}

// cleanupExpiredIterators removes expired iterators
func (im *IteratorManager) cleanupExpiredIterators() {
	im.mu.Lock()
	defer im.mu.Unlock()

	now := time.Now()
	expiredThreshold := 30 * time.Minute

	for id, metadata := range im.iterators {
		if now.Sub(metadata.LastAccessAt) > expiredThreshold {
			metadata.Status = IteratorStatusClosed
			delete(im.iterators, id)
			im.activeIterators--
		}
	}

	im.iteratorStats.CurrentActive = im.activeIterators
}

// SetMaxActiveIterators sets the maximum number of active iterators
func (im *IteratorManager) SetMaxActiveIterators(max uint32) {
	im.mu.Lock()
	defer im.mu.Unlock()
	im.maxActiveIters = max
}

// ResetStatistics resets global iterator statistics
func (im *IteratorManager) ResetStatistics() {
	im.mu.Lock()
	defer im.mu.Unlock()

	im.iteratorStats = &IteratorStatistics{
		CurrentActive:    im.activeIterators,
		TypeDistribution: make(map[string]uint64),
		PerformanceStats: &GlobalIterPerf{
			LastResetTime: time.Now(),
		},
	}
}
