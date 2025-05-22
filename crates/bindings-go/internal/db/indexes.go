package db

import (
	"fmt"
	"sort"
	"sync"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
)

// IndexManager manages index operations and metadata
type IndexManager struct {
	mu           sync.RWMutex
	indexes      map[IndexID]*IndexMetadataExt
	nameToID     map[string]IndexID
	tableIndexes map[TableID][]IndexID
	runtime      *runtime.Runtime
	nextIndexID  IndexID
	indexStats   map[IndexID]*IndexStatistics
}

// IndexMetadataExt extends IndexMetadata with additional operational fields
type IndexMetadataExt struct {
	IndexMetadata
	TableID     TableID          `json:"table_id"`
	Status      IndexStatus      `json:"status"`
	Size        uint64           `json:"size"`
	Cardinality uint64           `json:"cardinality"`
	Statistics  *IndexStatistics `json:"statistics"`
	Options     *IndexOptions    `json:"options"`
	Runtime     *runtime.Runtime `json:"-"`
}

// IndexStatus represents the status of an index
type IndexStatus int

const (
	IndexStatusCreating IndexStatus = iota
	IndexStatusActive
	IndexStatusDropping
	IndexStatusError
	IndexStatusMaintenance
)

func (is IndexStatus) String() string {
	switch is {
	case IndexStatusCreating:
		return "creating"
	case IndexStatusActive:
		return "active"
	case IndexStatusDropping:
		return "dropping"
	case IndexStatusError:
		return "error"
	case IndexStatusMaintenance:
		return "maintenance"
	default:
		return "unknown"
	}
}

// IndexOptions contains index configuration options
type IndexOptions struct {
	Unique         bool                   `json:"unique"`
	Algorithm      IndexAlgorithm         `json:"algorithm"`
	FillFactor     float64                `json:"fill_factor"`
	CacheSize      uint64                 `json:"cache_size"`
	Compression    bool                   `json:"compression"`
	Persistent     bool                   `json:"persistent"`
	Properties     map[string]interface{} `json:"properties"`
	ScanDirection  ScanDirection          `json:"scan_direction"`
	BloomFilter    bool                   `json:"bloom_filter"`
	PartitionCount uint32                 `json:"partition_count"`
}

// IndexAlgorithm represents different index algorithms
type IndexAlgorithm int

const (
	IndexAlgoBTree IndexAlgorithm = iota
	IndexAlgoHash
	IndexAlgoGin
	IndexAlgoGist
	IndexAlgoBitmap
	IndexAlgoRTree
)

func (ia IndexAlgorithm) String() string {
	switch ia {
	case IndexAlgoBTree:
		return "btree"
	case IndexAlgoHash:
		return "hash"
	case IndexAlgoGin:
		return "gin"
	case IndexAlgoGist:
		return "gist"
	case IndexAlgoBitmap:
		return "bitmap"
	case IndexAlgoRTree:
		return "rtree"
	default:
		return "btree"
	}
}

// ScanDirection represents index scan direction
type ScanDirection int

const (
	ScanDirectionForward ScanDirection = iota
	ScanDirectionBackward
	ScanDirectionBoth
)

// IndexStatistics contains index performance statistics
type IndexStatistics struct {
	ScanCount       uint64            `json:"scan_count"`
	LookupCount     uint64            `json:"lookup_count"`
	InsertCount     uint64            `json:"insert_count"`
	UpdateCount     uint64            `json:"update_count"`
	DeleteCount     uint64            `json:"delete_count"`
	HitRatio        float64           `json:"hit_ratio"`
	MissCount       uint64            `json:"miss_count"`
	LastUsedTime    time.Time         `json:"last_used_time"`
	AverageScanTime time.Duration     `json:"average_scan_time"`
	TotalScanTime   time.Duration     `json:"total_scan_time"`
	KeyDistribution map[string]uint64 `json:"key_distribution"`
	Performance     *IndexPerformance `json:"performance"`
}

// IndexPerformance tracks detailed performance metrics
type IndexPerformance struct {
	SelectivityRatio   float64 `json:"selectivity_ratio"`
	CacheHitRatio      float64 `json:"cache_hit_ratio"`
	AverageKeyLength   float64 `json:"average_key_length"`
	MaxKeyLength       uint32  `json:"max_key_length"`
	TreeHeight         uint32  `json:"tree_height"`
	LeafPages          uint64  `json:"leaf_pages"`
	InternalPages      uint64  `json:"internal_pages"`
	FragmentationRatio float64 `json:"fragmentation_ratio"`
}

// IndexScanRange represents a range for index scanning
type IndexScanRange struct {
	Lower          []byte        `json:"lower"`
	Upper          []byte        `json:"upper"`
	LowerInclusive bool          `json:"lower_inclusive"`
	UpperInclusive bool          `json:"upper_inclusive"`
	Limit          uint32        `json:"limit"`
	Offset         uint32        `json:"offset"`
	Direction      ScanDirection `json:"direction"`
}

// IndexBuilder helps build indexes
type IndexBuilder struct {
	name       string
	tableID    TableID
	columns    []string
	columnIDs  []uint32
	options    *IndexOptions
	algorithm  IndexAlgorithm
	unique     bool
	properties map[string]interface{}
}

// NewIndexManager creates a new index manager
func NewIndexManager(runtime *runtime.Runtime) *IndexManager {
	return &IndexManager{
		indexes:      make(map[IndexID]*IndexMetadataExt),
		nameToID:     make(map[string]IndexID),
		tableIndexes: make(map[TableID][]IndexID),
		runtime:      runtime,
		nextIndexID:  1,
		indexStats:   make(map[IndexID]*IndexStatistics),
	}
}

// CreateIndex creates a new index
func (im *IndexManager) CreateIndex(tableID TableID, name string, columns []string, options *IndexOptions) (*IndexMetadataExt, error) {
	im.mu.Lock()
	defer im.mu.Unlock()

	// Check if index already exists
	if _, exists := im.nameToID[name]; exists {
		return nil, fmt.Errorf("index %s already exists", name)
	}

	// Generate new index ID
	indexID := im.nextIndexID
	im.nextIndexID++

	// Set default options if not provided
	if options == nil {
		options = &IndexOptions{
			Algorithm:  IndexAlgoBTree,
			FillFactor: 0.9,
			Persistent: true,
			Properties: make(map[string]interface{}),
		}
	}

	// Create index metadata
	metadata := &IndexMetadataExt{
		IndexMetadata: IndexMetadata{
			ID:        indexID,
			Name:      name,
			Type:      options.Algorithm.String(),
			Columns:   columns,
			ColumnIDs: im.getColumnIDs(tableID, columns),
			Unique:    options.Unique,
			Algorithm: options.Algorithm.String(),
			CreatedAt: time.Now(),
		},
		TableID:     tableID,
		Status:      IndexStatusCreating,
		Size:        0,
		Cardinality: 0,
		Statistics: &IndexStatistics{
			KeyDistribution: make(map[string]uint64),
			Performance:     &IndexPerformance{},
		},
		Options: options,
		Runtime: im.runtime,
	}

	// Validate index
	if err := im.validateIndex(metadata); err != nil {
		return nil, fmt.Errorf("invalid index: %w", err)
	}

	// Store metadata
	im.indexes[indexID] = metadata
	im.nameToID[name] = indexID
	im.indexStats[indexID] = metadata.Statistics

	// Add to table index list
	im.tableIndexes[tableID] = append(im.tableIndexes[tableID], indexID)

	// Mark as active after creation
	metadata.Status = IndexStatusActive

	return metadata, nil
}

// GetIndex returns index metadata by ID
func (im *IndexManager) GetIndex(indexID IndexID) (*IndexMetadataExt, error) {
	im.mu.RLock()
	defer im.mu.RUnlock()

	metadata, exists := im.indexes[indexID]
	if !exists {
		return nil, NewErrno(ErrNoSuchIndex)
	}

	return metadata, nil
}

// GetIndexByName returns index metadata by name
func (im *IndexManager) GetIndexByName(name string) (*IndexMetadataExt, error) {
	im.mu.RLock()
	defer im.mu.RUnlock()

	indexID, exists := im.nameToID[name]
	if !exists {
		return nil, NewErrno(ErrNoSuchIndex)
	}

	return im.indexes[indexID], nil
}

// GetTableIndexes returns all indexes for a table
func (im *IndexManager) GetTableIndexes(tableID TableID) ([]*IndexMetadataExt, error) {
	im.mu.RLock()
	defer im.mu.RUnlock()

	indexIDs, exists := im.tableIndexes[tableID]
	if !exists {
		return []*IndexMetadataExt{}, nil
	}

	indexes := make([]*IndexMetadataExt, 0, len(indexIDs))
	for _, indexID := range indexIDs {
		if metadata, exists := im.indexes[indexID]; exists {
			indexes = append(indexes, metadata)
		}
	}

	return indexes, nil
}

// DropIndex removes an index
func (im *IndexManager) DropIndex(indexID IndexID) error {
	im.mu.Lock()
	defer im.mu.Unlock()

	metadata, exists := im.indexes[indexID]
	if !exists {
		return NewErrno(ErrNoSuchIndex)
	}

	// Mark as dropping
	metadata.Status = IndexStatusDropping

	// Remove from maps
	delete(im.indexes, indexID)
	delete(im.nameToID, metadata.Name)
	delete(im.indexStats, indexID)

	// Remove from table index list
	if indexIDs, exists := im.tableIndexes[metadata.TableID]; exists {
		for i, id := range indexIDs {
			if id == indexID {
				im.tableIndexes[metadata.TableID] = append(indexIDs[:i], indexIDs[i+1:]...)
				break
			}
		}
	}

	return nil
}

// ScanIndex creates an iterator for scanning an index range
func (im *IndexManager) ScanIndex(indexID IndexID, scanRange *IndexScanRange) (*RowIter, error) {
	im.mu.RLock()
	metadata, exists := im.indexes[indexID]
	im.mu.RUnlock()

	if !exists {
		return nil, NewErrno(ErrNoSuchIndex)
	}

	// Update statistics
	im.updateIndexStatistics(indexID, IndexOpScan, time.Now())

	// Perform the actual scan based on algorithm
	return im.performIndexScan(metadata, scanRange)
}

// performIndexScan performs the actual index scan
func (im *IndexManager) performIndexScan(metadata *IndexMetadataExt, scanRange *IndexScanRange) (*RowIter, error) {
	switch metadata.Options.Algorithm {
	case IndexAlgoBTree:
		return im.btreeScan(metadata, scanRange)
	case IndexAlgoHash:
		return im.hashScan(metadata, scanRange)
	default:
		return im.btreeScan(metadata, scanRange)
	}
}

// btreeScan performs B-tree index scan
func (im *IndexManager) btreeScan(metadata *IndexMetadataExt, scanRange *IndexScanRange) (*RowIter, error) {
	// Simplified B-tree scan implementation
	// In a real implementation, this would use the actual B-tree structure

	// Create iterator with range constraints
	iter := &RowIter{
		data:    [][]byte{}, // Would be populated from actual index scan
		idx:     0,
		IterID:  uint32(metadata.ID),
		Runtime: metadata.Runtime,
	}

	return iter, nil
}

// hashScan performs hash index scan
func (im *IndexManager) hashScan(metadata *IndexMetadataExt, scanRange *IndexScanRange) (*RowIter, error) {
	// Hash indexes only support equality lookups
	if scanRange.Lower == nil || len(scanRange.Lower) == 0 {
		return nil, fmt.Errorf("hash index requires exact key for lookup")
	}

	// Simplified hash scan implementation
	iter := &RowIter{
		data:    [][]byte{}, // Would be populated from hash lookup
		idx:     0,
		IterID:  uint32(metadata.ID),
		Runtime: metadata.Runtime,
	}

	return iter, nil
}

// UpdateIndexStatistics updates index performance statistics
func (im *IndexManager) UpdateIndexStatistics(indexID IndexID, operation IndexOperation, duration time.Duration, keyCount uint32, success bool) {
	im.mu.Lock()
	defer im.mu.Unlock()

	stats, exists := im.indexStats[indexID]
	if !exists {
		return
	}

	stats.LastUsedTime = time.Now()

	switch operation {
	case IndexOpScan:
		stats.ScanCount++
		stats.TotalScanTime += duration
		stats.AverageScanTime = time.Duration(int64(stats.TotalScanTime) / int64(stats.ScanCount))
	case IndexOpLookup:
		stats.LookupCount++
		if success {
			// Calculate hit ratio
			totalOperations := stats.LookupCount + stats.MissCount
			stats.HitRatio = float64(stats.LookupCount) / float64(totalOperations)
		} else {
			stats.MissCount++
		}
	case IndexOpInsert:
		stats.InsertCount++
	case IndexOpUpdate:
		stats.UpdateCount++
	case IndexOpDelete:
		stats.DeleteCount++
	}
}

// updateIndexStatistics is a simplified version for internal use
func (im *IndexManager) updateIndexStatistics(indexID IndexID, operation IndexOperation, timestamp time.Time) {
	im.UpdateIndexStatistics(indexID, operation, 0, 0, true)
}

// IndexOperation represents index operations
type IndexOperation int

const (
	IndexOpScan IndexOperation = iota
	IndexOpLookup
	IndexOpInsert
	IndexOpUpdate
	IndexOpDelete
)

// GetIndexStatistics returns index statistics
func (im *IndexManager) GetIndexStatistics(indexID IndexID) (*IndexStatistics, error) {
	im.mu.RLock()
	defer im.mu.RUnlock()

	stats, exists := im.indexStats[indexID]
	if !exists {
		return nil, NewErrno(ErrNoSuchIndex)
	}

	return stats, nil
}

// RebuildIndex rebuilds an index
func (im *IndexManager) RebuildIndex(indexID IndexID) error {
	im.mu.Lock()
	defer im.mu.Unlock()

	metadata, exists := im.indexes[indexID]
	if !exists {
		return NewErrno(ErrNoSuchIndex)
	}

	// Mark as maintenance
	metadata.Status = IndexStatusMaintenance

	// Simulate rebuild process
	// In a real implementation, this would rebuild the index structure
	time.Sleep(10 * time.Millisecond) // Simulate work

	// Mark as active
	metadata.Status = IndexStatusActive

	// Reset some statistics
	if stats := im.indexStats[indexID]; stats != nil {
		stats.Performance.FragmentationRatio = 0.0
	}

	return nil
}

// OptimizeIndex optimizes an index
func (im *IndexManager) OptimizeIndex(indexID IndexID) error {
	im.mu.Lock()
	defer im.mu.Unlock()

	metadata, exists := im.indexes[indexID]
	if !exists {
		return NewErrno(ErrNoSuchIndex)
	}

	// Perform optimization based on algorithm
	switch metadata.Options.Algorithm {
	case IndexAlgoBTree:
		return im.optimizeBTreeIndex(metadata)
	case IndexAlgoHash:
		return im.optimizeHashIndex(metadata)
	default:
		return im.optimizeBTreeIndex(metadata)
	}
}

// optimizeBTreeIndex optimizes a B-tree index
func (im *IndexManager) optimizeBTreeIndex(metadata *IndexMetadataExt) error {
	// Simulate B-tree optimization
	if stats := im.indexStats[metadata.ID]; stats != nil {
		stats.Performance.FragmentationRatio *= 0.5 // Reduce fragmentation
	}
	return nil
}

// optimizeHashIndex optimizes a hash index
func (im *IndexManager) optimizeHashIndex(metadata *IndexMetadataExt) error {
	// Simulate hash index optimization
	if stats := im.indexStats[metadata.ID]; stats != nil {
		stats.Performance.CacheHitRatio = 0.95 // Improve cache hit ratio
	}
	return nil
}

// ListIndexes returns all indexes
func (im *IndexManager) ListIndexes() []*IndexMetadataExt {
	im.mu.RLock()
	defer im.mu.RUnlock()

	indexes := make([]*IndexMetadataExt, 0, len(im.indexes))
	for _, metadata := range im.indexes {
		indexes = append(indexes, metadata)
	}

	// Sort by name for consistent ordering
	sort.Slice(indexes, func(i, j int) bool {
		return indexes[i].Name < indexes[j].Name
	})

	return indexes
}

// validateIndex validates index configuration
func (im *IndexManager) validateIndex(metadata *IndexMetadataExt) error {
	if metadata.Name == "" {
		return fmt.Errorf("index name cannot be empty")
	}

	if len(metadata.Columns) == 0 {
		return fmt.Errorf("index must specify at least one column")
	}

	// Validate algorithm-specific constraints
	switch metadata.Options.Algorithm {
	case IndexAlgoHash:
		if len(metadata.Columns) > 1 {
			return fmt.Errorf("hash index supports only single column")
		}
	case IndexAlgoBTree:
		// B-tree supports multiple columns
	default:
		return fmt.Errorf("unsupported index algorithm: %s", metadata.Options.Algorithm.String())
	}

	return nil
}

// getColumnIDs converts column names to column IDs
func (im *IndexManager) getColumnIDs(tableID TableID, columns []string) []uint32 {
	// Simplified implementation - would normally look up actual column IDs
	columnIDs := make([]uint32, len(columns))
	for i := range columns {
		columnIDs[i] = uint32(i)
	}
	return columnIDs
}

// NewIndexBuilder creates a new index builder
func NewIndexBuilder(name string, tableID TableID) *IndexBuilder {
	return &IndexBuilder{
		name:       name,
		tableID:    tableID,
		columns:    []string{},
		columnIDs:  []uint32{},
		algorithm:  IndexAlgoBTree,
		unique:     false,
		properties: make(map[string]interface{}),
		options: &IndexOptions{
			Algorithm:  IndexAlgoBTree,
			FillFactor: 0.9,
			Persistent: true,
			Properties: make(map[string]interface{}),
		},
	}
}

// AddColumn adds a column to the index
func (ib *IndexBuilder) AddColumn(column string) *IndexBuilder {
	ib.columns = append(ib.columns, column)
	ib.columnIDs = append(ib.columnIDs, uint32(len(ib.columnIDs)))
	return ib
}

// SetUnique marks the index as unique
func (ib *IndexBuilder) SetUnique(unique bool) *IndexBuilder {
	ib.unique = unique
	ib.options.Unique = unique
	return ib
}

// SetAlgorithm sets the index algorithm
func (ib *IndexBuilder) SetAlgorithm(algorithm IndexAlgorithm) *IndexBuilder {
	ib.algorithm = algorithm
	ib.options.Algorithm = algorithm
	return ib
}

// SetFillFactor sets the index fill factor
func (ib *IndexBuilder) SetFillFactor(fillFactor float64) *IndexBuilder {
	ib.options.FillFactor = fillFactor
	return ib
}

// SetCacheSize sets the index cache size
func (ib *IndexBuilder) SetCacheSize(cacheSize uint64) *IndexBuilder {
	ib.options.CacheSize = cacheSize
	return ib
}

// SetProperty sets an index property
func (ib *IndexBuilder) SetProperty(key string, value interface{}) *IndexBuilder {
	ib.properties[key] = value
	ib.options.Properties[key] = value
	return ib
}

// Build builds the index using the index manager
func (ib *IndexBuilder) Build(im *IndexManager) (*IndexMetadataExt, error) {
	return im.CreateIndex(ib.tableID, ib.name, ib.columns, ib.options)
}

// IndexAnalyzer provides index analysis utilities
type IndexAnalyzer struct {
	indexManager *IndexManager
}

// NewIndexAnalyzer creates a new index analyzer
func NewIndexAnalyzer(indexManager *IndexManager) *IndexAnalyzer {
	return &IndexAnalyzer{
		indexManager: indexManager,
	}
}

// AnalyzeIndexUsage analyzes index usage patterns
func (ia *IndexAnalyzer) AnalyzeIndexUsage(tableID TableID) ([]*IndexUsageReport, error) {
	indexes, err := ia.indexManager.GetTableIndexes(tableID)
	if err != nil {
		return nil, err
	}

	reports := make([]*IndexUsageReport, 0, len(indexes))
	for _, index := range indexes {
		stats, err := ia.indexManager.GetIndexStatistics(index.ID)
		if err != nil {
			continue
		}

		report := &IndexUsageReport{
			IndexID:     index.ID,
			IndexName:   index.Name,
			Usage:       stats.ScanCount + stats.LookupCount,
			Efficiency:  stats.HitRatio,
			LastUsed:    stats.LastUsedTime,
			Recommended: ia.getRecommendation(stats),
		}
		reports = append(reports, report)
	}

	// Sort by usage
	sort.Slice(reports, func(i, j int) bool {
		return reports[i].Usage > reports[j].Usage
	})

	return reports, nil
}

// IndexUsageReport contains index usage analysis
type IndexUsageReport struct {
	IndexID     IndexID   `json:"index_id"`
	IndexName   string    `json:"index_name"`
	Usage       uint64    `json:"usage"`
	Efficiency  float64   `json:"efficiency"`
	LastUsed    time.Time `json:"last_used"`
	Recommended string    `json:"recommended"`
}

// getRecommendation provides optimization recommendations
func (ia *IndexAnalyzer) getRecommendation(stats *IndexStatistics) string {
	if stats.ScanCount == 0 && stats.LookupCount == 0 {
		return "CONSIDER_DROPPING"
	}
	if stats.HitRatio < 0.5 {
		return "OPTIMIZE_NEEDED"
	}
	if stats.Performance.FragmentationRatio > 0.3 {
		return "REBUILD_RECOMMENDED"
	}
	return "PERFORMING_WELL"
}

// SuggestIndexes suggests new indexes based on query patterns
func (ia *IndexAnalyzer) SuggestIndexes(tableID TableID, queryPatterns []string) ([]*IndexSuggestion, error) {
	// Simplified implementation - would analyze actual query patterns
	suggestions := []*IndexSuggestion{
		{
			TableID:       tableID,
			Columns:       []string{"id"},
			Reason:        "Primary key access pattern detected",
			Algorithm:     IndexAlgoBTree,
			Priority:      "HIGH",
			EstimatedGain: "Significant performance improvement for lookups",
		},
	}

	return suggestions, nil
}

// IndexSuggestion contains index creation suggestions
type IndexSuggestion struct {
	TableID       TableID        `json:"table_id"`
	Columns       []string       `json:"columns"`
	Reason        string         `json:"reason"`
	Algorithm     IndexAlgorithm `json:"algorithm"`
	Priority      string         `json:"priority"`
	EstimatedGain string         `json:"estimated_gain"`
}
