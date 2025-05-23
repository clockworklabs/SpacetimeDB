package db

import (
	"fmt"
	"testing"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Test helpers

func createTestRuntime() *runtime.Runtime {
	return &runtime.Runtime{}
}

func createTestTableManager() *TableManager {
	return NewTableManager(createTestRuntime())
}

func createTestIndexManager() *IndexManager {
	return NewIndexManager(createTestRuntime())
}

func createTestIteratorManager() *IteratorManager {
	return NewIteratorManager(createTestRuntime())
}

func createTestEncodingManager() *EncodingManager {
	return NewEncodingManager(createTestRuntime())
}

// TableManager Tests

func TestTableManager_CreateTable(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{
			ID:         0,
			Name:       "id",
			Type:       "uint32",
			PrimaryKey: true,
		},
		{
			ID:   1,
			Name: "name",
			Type: "string",
		},
	}

	metadata, err := tm.CreateTable("test_table", []byte("test schema"), columns)
	assert.NoError(t, err)
	assert.NotNil(t, metadata)
	assert.Equal(t, "test_table", metadata.Name)
	assert.Equal(t, TableID(1), metadata.ID)
	assert.Equal(t, len(columns), len(metadata.Columns))
}

func TestTableManager_CreateTableDuplicate(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create first table
	_, err := tm.CreateTable("test_table", []byte("schema"), columns)
	assert.NoError(t, err)

	// Try to create duplicate table
	_, err = tm.CreateTable("test_table", []byte("schema"), columns)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")
}

func TestTableManager_GetTable(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create table
	created, err := tm.CreateTable("test_table", []byte("schema"), columns)
	require.NoError(t, err)

	// Get table by ID
	retrieved, err := tm.GetTable(created.ID)
	assert.NoError(t, err)
	assert.Equal(t, created.Name, retrieved.Name)
	assert.Equal(t, created.ID, retrieved.ID)
}

func TestTableManager_GetTableByName(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create table
	created, err := tm.CreateTable("test_table", []byte("schema"), columns)
	require.NoError(t, err)

	// Get table by name
	retrieved, err := tm.GetTableByName("test_table")
	assert.NoError(t, err)
	assert.Equal(t, created.Name, retrieved.Name)
	assert.Equal(t, created.ID, retrieved.ID)
}

func TestTableManager_UpdateTableMetadata(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create table
	created, err := tm.CreateTable("test_table", []byte("schema"), columns)
	require.NoError(t, err)

	// Update metadata
	updates := map[string]interface{}{
		"access_level": "public",
		"description":  "Test table description",
	}

	err = tm.UpdateTableMetadata(created.ID, updates)
	assert.NoError(t, err)

	// Verify updates
	updated, err := tm.GetTable(created.ID)
	assert.NoError(t, err)
	assert.Equal(t, "public", updated.AccessLevel)
	assert.Equal(t, "Test table description", updated.Properties["description"])
}

func TestTableManager_DropTable(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create table
	created, err := tm.CreateTable("test_table", []byte("schema"), columns)
	require.NoError(t, err)

	// Drop table
	err = tm.DropTable(created.ID)
	assert.NoError(t, err)

	// Verify table is gone
	_, err = tm.GetTable(created.ID)
	assert.Error(t, err)
}

func TestTableManager_ListTables(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create multiple tables
	_, err := tm.CreateTable("table1", []byte("schema1"), columns)
	require.NoError(t, err)

	_, err = tm.CreateTable("table2", []byte("schema2"), columns)
	require.NoError(t, err)

	// List tables
	tables := tm.ListTables()
	assert.Len(t, tables, 2)
}

func TestTableManager_UpdateTableStatistics(t *testing.T) {
	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
	}

	// Create table
	created, err := tm.CreateTable("test_table", []byte("schema"), columns)
	require.NoError(t, err)

	// Update statistics
	tm.UpdateTableStatistics(created.ID, TableOpInsert, 100*time.Millisecond, 5, true)

	// Get statistics
	stats, err := tm.GetTableStatistics(created.ID)
	assert.NoError(t, err)
	assert.Equal(t, uint64(1), stats.InsertCount)
	assert.Equal(t, uint64(5), stats.RowCount)
}

// IndexManager Tests

func TestIndexManager_CreateIndex(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Unique:     true,
		Properties: make(map[string]interface{}),
	}

	metadata, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	assert.NoError(t, err)
	assert.NotNil(t, metadata)
	assert.Equal(t, "test_index", metadata.Name)
	assert.Equal(t, IndexID(1), metadata.ID)
	assert.True(t, metadata.Unique)
}

func TestIndexManager_CreateIndexDuplicate(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create first index
	_, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	assert.NoError(t, err)

	// Try to create duplicate index
	_, err = im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")
}

func TestIndexManager_GetIndex(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create index
	created, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	require.NoError(t, err)

	// Get index
	retrieved, err := im.GetIndex(created.ID)
	assert.NoError(t, err)
	assert.Equal(t, created.Name, retrieved.Name)
	assert.Equal(t, created.ID, retrieved.ID)
}

func TestIndexManager_GetIndexByName(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create index
	created, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	require.NoError(t, err)

	// Get index by name
	retrieved, err := im.GetIndexByName("test_index")
	assert.NoError(t, err)
	assert.Equal(t, created.Name, retrieved.Name)
	assert.Equal(t, created.ID, retrieved.ID)
}

func TestIndexManager_GetTableIndexes(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	tableID := TableID(1)

	// Create multiple indexes for the same table
	_, err := im.CreateIndex(tableID, "index1", []string{"id"}, options)
	require.NoError(t, err)

	_, err = im.CreateIndex(tableID, "index2", []string{"name"}, options)
	require.NoError(t, err)

	// Get table indexes
	indexes, err := im.GetTableIndexes(tableID)
	assert.NoError(t, err)
	assert.Len(t, indexes, 2)
}

func TestIndexManager_DropIndex(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create index
	created, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	require.NoError(t, err)

	// Drop index
	err = im.DropIndex(created.ID)
	assert.NoError(t, err)

	// Verify index is gone
	_, err = im.GetIndex(created.ID)
	assert.Error(t, err)
}

func TestIndexManager_ScanIndex(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create index
	created, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	require.NoError(t, err)

	// Create scan range
	scanRange := &IndexScanRange{
		Lower:          []byte("1"),
		Upper:          []byte("10"),
		LowerInclusive: true,
		UpperInclusive: true,
		Limit:          100,
		Offset:         0,
		Direction:      ScanDirectionForward,
	}

	// Scan index
	iter, err := im.ScanIndex(created.ID, scanRange)
	assert.NoError(t, err)
	assert.NotNil(t, iter)
}

func TestIndexManager_UpdateIndexStatistics(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create index
	created, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	require.NoError(t, err)

	// Update statistics
	im.UpdateIndexStatistics(created.ID, IndexOpScan, 50*time.Millisecond, 10, true)

	// Get statistics
	stats, err := im.GetIndexStatistics(created.ID)
	assert.NoError(t, err)
	assert.Equal(t, uint64(1), stats.ScanCount)
}

func TestIndexManager_RebuildIndex(t *testing.T) {
	im := createTestIndexManager()

	options := &IndexOptions{
		Algorithm:  IndexAlgoBTree,
		Properties: make(map[string]interface{}),
	}

	// Create index
	created, err := im.CreateIndex(TableID(1), "test_index", []string{"id"}, options)
	require.NoError(t, err)

	// Rebuild index
	err = im.RebuildIndex(created.ID)
	assert.NoError(t, err)

	// Verify index is still active
	retrieved, err := im.GetIndex(created.ID)
	assert.NoError(t, err)
	assert.Equal(t, IndexStatusActive, retrieved.Status)
}

// IteratorManager Tests

func TestIteratorManager_CreateTableIterator(t *testing.T) {
	iterMgr := createTestIteratorManager()

	options := &IteratorOptions{
		BatchSize:  50,
		Timeout:    10 * time.Second,
		Prefetch:   true,
		Properties: make(map[string]interface{}),
	}

	iter, err := iterMgr.CreateTableIterator(TableID(1), options)
	assert.NoError(t, err)
	assert.NotNil(t, iter)
	assert.Equal(t, IteratorTypeTableScan, iter.metadata.Type)
	assert.Equal(t, TableID(1), iter.metadata.TableID)
}

func TestIteratorManager_CreateIndexIterator(t *testing.T) {
	iterMgr := createTestIteratorManager()

	scanRange := &IndexScanRange{
		Lower:          []byte("1"),
		Upper:          []byte("10"),
		LowerInclusive: true,
		UpperInclusive: true,
		Direction:      ScanDirectionForward,
	}

	options := &IteratorOptions{
		BatchSize:  50,
		Timeout:    10 * time.Second,
		Properties: make(map[string]interface{}),
	}

	iter, err := iterMgr.CreateIndexIterator(IndexID(1), scanRange, options)
	assert.NoError(t, err)
	assert.NotNil(t, iter)
	assert.Equal(t, IteratorTypeIndexScan, iter.metadata.Type)
	assert.Equal(t, IndexID(1), iter.metadata.IndexID)
}

func TestIteratorManager_CreateBatchIterator(t *testing.T) {
	iterMgr := createTestIteratorManager()

	// Create base iterator
	baseIter, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	// Create batch iterator
	batchIter := iterMgr.CreateBatchIterator(baseIter, 10)
	assert.NotNil(t, batchIter)
	assert.Equal(t, uint32(10), batchIter.batchSize)
}

func TestIteratorManager_CreateStreamIterator(t *testing.T) {
	iterMgr := createTestIteratorManager()

	// Create base iterator
	baseIter, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	// Create stream iterator
	streamIter := iterMgr.CreateStreamIterator(baseIter, 100)
	assert.NotNil(t, streamIter)
	assert.Equal(t, 100, streamIter.bufferSize)
}

func TestIteratorManager_GetIteratorStatistics(t *testing.T) {
	iterMgr := createTestIteratorManager()

	// Create some iterators
	_, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	_, err = iterMgr.CreateTableIterator(TableID(2), nil)
	require.NoError(t, err)

	// Get statistics
	stats := iterMgr.GetIteratorStatistics()
	assert.NotNil(t, stats)
	assert.Equal(t, uint64(2), stats.TotalCreated)
	assert.Equal(t, uint32(2), stats.CurrentActive)
}

func TestIteratorManager_CloseAllIterators(t *testing.T) {
	iterMgr := createTestIteratorManager()

	// Create some iterators
	_, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	_, err = iterMgr.CreateTableIterator(TableID(2), nil)
	require.NoError(t, err)

	// Close all iterators
	err = iterMgr.CloseAllIterators()
	assert.NoError(t, err)

	// Verify all are closed
	stats := iterMgr.GetIteratorStatistics()
	assert.Equal(t, uint32(0), stats.CurrentActive)
}

func TestEnhancedRowIter_Read(t *testing.T) {
	iterMgr := createTestIteratorManager()

	iter, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	// Read should not error even if no data (returns exhausted)
	_, err = iter.Read()
	assert.Error(t, err) // Should be "iterator exhausted"
}

func TestEnhancedRowIter_Close(t *testing.T) {
	iterMgr := createTestIteratorManager()

	iter, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	// Close iterator
	err = iter.Close()
	assert.NoError(t, err)

	// Verify status
	assert.Equal(t, IteratorStatusClosed, iter.metadata.Status)
}

func TestBatchIterator_ReadBatch(t *testing.T) {
	iterMgr := createTestIteratorManager()

	baseIter, err := iterMgr.CreateTableIterator(TableID(1), nil)
	require.NoError(t, err)

	batchIter := iterMgr.CreateBatchIterator(baseIter, 5)

	// Try to read batch (should return EOF since no data)
	batch, err := batchIter.ReadBatch()
	assert.Error(t, err) // Should be EOF
	assert.Nil(t, batch)
}

// EncodingManager Tests

func TestEncodingManager_Encode(t *testing.T) {
	em := createTestEncodingManager()

	data := map[string]interface{}{
		"id":   uint32(123),
		"name": "test",
	}

	options := &EncodingOptions{
		Format:     EncodingBSATN,
		Properties: make(map[string]interface{}),
	}

	encoded, err := em.Encode(data, EncodingBSATN, options)
	assert.NoError(t, err)
	assert.NotNil(t, encoded)
	assert.Greater(t, len(encoded), 0)
}

func TestEncodingManager_Decode(t *testing.T) {
	em := createTestEncodingManager()

	// Define a test struct for decoding
	type TestStruct struct {
		ID   uint32 `json:"id"`
		Name string `json:"name"`
	}

	// First encode some data
	originalData := TestStruct{
		ID:   uint32(123),
		Name: "test",
	}

	encoded, err := em.Encode(originalData, EncodingBSATN, nil)
	require.NoError(t, err)

	// Then decode it
	var decodedData TestStruct
	options := &DecodingOptions{
		Format:     EncodingBSATN,
		Properties: make(map[string]interface{}),
	}

	err = em.Decode(encoded, &decodedData, EncodingBSATN, options)
	assert.NoError(t, err)
	assert.Equal(t, originalData.ID, decodedData.ID)
	assert.Equal(t, originalData.Name, decodedData.Name)
}

func TestEncodingManager_EncodeWithCompression(t *testing.T) {
	em := createTestEncodingManager()

	data := map[string]interface{}{
		"id":   uint32(123),
		"name": "test data that should compress well when repeated multiple times",
	}

	options := &EncodingOptions{
		Format:           EncodingBSATN,
		Compression:      CompressionGzip,
		CompressionLevel: 6,
		Properties:       make(map[string]interface{}),
	}

	encoded, err := em.Encode(data, EncodingBSATN, options)
	assert.NoError(t, err)
	assert.NotNil(t, encoded)
	assert.Greater(t, len(encoded), 0)
}

func TestEncodingManager_DecodeWithDecompression(t *testing.T) {
	em := createTestEncodingManager()

	// Define a test struct for decoding
	type TestStruct struct {
		ID   uint32 `json:"id"`
		Name string `json:"name"`
	}

	// Encode with compression
	originalData := TestStruct{
		ID:   uint32(123),
		Name: "test",
	}

	encodeOptions := &EncodingOptions{
		Format:      EncodingBSATN,
		Compression: CompressionGzip,
		Properties:  make(map[string]interface{}),
	}

	encoded, err := em.Encode(originalData, EncodingBSATN, encodeOptions)
	require.NoError(t, err)

	// Decode with decompression
	var decodedData TestStruct
	decodeOptions := &DecodingOptions{
		Format:      EncodingBSATN,
		Compression: CompressionGzip,
		Properties:  make(map[string]interface{}),
	}

	err = em.Decode(encoded, &decodedData, EncodingBSATN, decodeOptions)
	assert.NoError(t, err)
	assert.Equal(t, originalData.ID, decodedData.ID)
	assert.Equal(t, originalData.Name, decodedData.Name)
}

func TestEncodingManager_RegisterSchema(t *testing.T) {
	em := createTestEncodingManager()

	schema := &SchemaInfo{
		ID:         "test_schema",
		Version:    1,
		Format:     EncodingBSATN,
		Schema:     []byte("test schema data"),
		CreatedAt:  time.Now(),
		UpdatedAt:  time.Now(),
		Properties: make(map[string]interface{}),
	}

	err := em.RegisterSchema(schema)
	assert.NoError(t, err)
}

func TestEncodingManager_GetStatistics(t *testing.T) {
	em := createTestEncodingManager()

	// Perform some operations
	data := map[string]interface{}{"test": "data"}
	_, err := em.Encode(data, EncodingBSATN, nil)
	require.NoError(t, err)

	// Get statistics
	stats := em.GetStatistics()
	assert.NotNil(t, stats)
	assert.Equal(t, uint64(1), stats.TotalEncoded)
}

// BSATNEncoder Tests

func TestBSATNEncoder_Encode(t *testing.T) {
	encoder := &BSATNEncoder{
		options: &EncodingOptions{
			Format:     EncodingBSATN,
			Properties: make(map[string]interface{}),
		},
	}

	data := map[string]interface{}{
		"id":   uint32(123),
		"name": "test",
	}

	encoded, err := encoder.Encode(data)
	assert.NoError(t, err)
	assert.NotNil(t, encoded)
	assert.Greater(t, len(encoded), 0)
}

func TestBSATNEncoder_GetFormat(t *testing.T) {
	encoder := &BSATNEncoder{
		options: &EncodingOptions{
			Format: EncodingBSATN,
		},
	}

	format := encoder.GetFormat()
	assert.Equal(t, EncodingBSATN, format)
}

// BSATNDecoder Tests

func TestBSATNDecoder_Decode(t *testing.T) {
	// Define a test struct for decoding
	type TestStruct struct {
		ID   uint32 `json:"id"`
		Name string `json:"name"`
	}

	// First encode some data
	encoder := &BSATNEncoder{
		options: &EncodingOptions{
			Format: EncodingBSATN,
		},
	}

	originalData := TestStruct{
		ID:   uint32(123),
		Name: "test",
	}

	encoded, err := encoder.Encode(originalData)
	require.NoError(t, err)

	// Then decode it
	decoder := &BSATNDecoder{
		options: &DecodingOptions{
			Format: EncodingBSATN,
		},
	}

	var decodedData TestStruct
	err = decoder.Decode(encoded, &decodedData)
	assert.NoError(t, err)
	assert.Equal(t, originalData.ID, decodedData.ID)
	assert.Equal(t, originalData.Name, decodedData.Name)
}

func TestBSATNDecoder_GetFormat(t *testing.T) {
	decoder := &BSATNDecoder{
		options: &DecodingOptions{
			Format: EncodingBSATN,
		},
	}

	format := decoder.GetFormat()
	assert.Equal(t, EncodingBSATN, format)
}

// GzipCompressor Tests

func TestGzipCompressor_Compress(t *testing.T) {
	compressor := &GzipCompressor{level: 6}

	data := []byte("test data that should compress well when repeated multiple times")

	compressed, err := compressor.Compress(data)
	assert.NoError(t, err)
	assert.NotNil(t, compressed)
	assert.Greater(t, len(compressed), 0)
}

func TestGzipCompressor_Decompress(t *testing.T) {
	compressor := &GzipCompressor{level: 6}

	originalData := []byte("test data for compression")

	// Compress
	compressed, err := compressor.Compress(originalData)
	require.NoError(t, err)

	// Decompress
	decompressed, err := compressor.Decompress(compressed)
	assert.NoError(t, err)
	assert.Equal(t, originalData, decompressed)
}

func TestGzipCompressor_GetType(t *testing.T) {
	compressor := &GzipCompressor{level: 6}

	compressionType := compressor.GetType()
	assert.Equal(t, CompressionGzip, compressionType)
}

func TestGzipCompressor_GetLevel(t *testing.T) {
	compressor := &GzipCompressor{level: 9}

	level := compressor.GetLevel()
	assert.Equal(t, 9, level)
}

// Integration Tests

func TestDatabaseOperations_Integration(t *testing.T) {
	// Create managers
	tm := createTestTableManager()
	im := createTestIndexManager()
	iterMgr := createTestIteratorManager()
	em := createTestEncodingManager()

	// Create a table
	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32", PrimaryKey: true},
		{ID: 1, Name: "name", Type: "string"},
		{ID: 2, Name: "age", Type: "uint32"},
	}

	tableMetadata, err := tm.CreateTable("users", []byte("users table schema"), columns)
	require.NoError(t, err)

	// Create indexes
	primaryIndex, err := im.CreateIndex(tableMetadata.ID, "users_pk", []string{"id"}, &IndexOptions{
		Algorithm: IndexAlgoBTree,
		Unique:    true,
	})
	require.NoError(t, err)

	nameIndex, err := im.CreateIndex(tableMetadata.ID, "users_name_idx", []string{"name"}, &IndexOptions{
		Algorithm: IndexAlgoBTree,
		Unique:    false,
	})
	require.NoError(t, err)

	// Create iterator for table scan
	tableIter, err := iterMgr.CreateTableIterator(tableMetadata.ID, &IteratorOptions{
		BatchSize: 100,
		Prefetch:  true,
	})
	require.NoError(t, err)

	// Create iterator for index scan
	scanRange := &IndexScanRange{
		Lower:          []byte("A"),
		Upper:          []byte("Z"),
		LowerInclusive: true,
		UpperInclusive: true,
		Direction:      ScanDirectionForward,
	}

	indexIter, err := iterMgr.CreateIndexIterator(nameIndex.ID, scanRange, &IteratorOptions{
		BatchSize: 50,
	})
	require.NoError(t, err)

	// Test encoding/decoding
	type UserStruct struct {
		ID   uint32 `json:"id"`
		Name string `json:"name"`
		Age  uint32 `json:"age"`
	}

	testData := UserStruct{
		ID:   uint32(1),
		Name: "John Doe",
		Age:  uint32(30),
	}

	encoded, err := em.Encode(testData, EncodingBSATN, &EncodingOptions{
		Compression: CompressionGzip,
	})
	require.NoError(t, err)

	var decoded UserStruct
	err = em.Decode(encoded, &decoded, EncodingBSATN, &DecodingOptions{
		Compression: CompressionGzip,
	})
	require.NoError(t, err)

	// Verify everything works together
	assert.NotNil(t, tableMetadata)
	assert.NotNil(t, primaryIndex)
	assert.NotNil(t, nameIndex)
	assert.NotNil(t, tableIter)
	assert.NotNil(t, indexIter)
	assert.NotNil(t, encoded)
	assert.NotNil(t, decoded)

	// Clean up
	err = tableIter.Close()
	assert.NoError(t, err)

	err = indexIter.Close()
	assert.NoError(t, err)
}

// Performance Tests

func TestTableManager_Performance(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping performance test in short mode")
	}

	tm := createTestTableManager()

	columns := []ColumnMetadata{
		{ID: 0, Name: "id", Type: "uint32"},
		{ID: 1, Name: "data", Type: "string"},
	}

	// Create many tables
	startTime := time.Now()
	for i := 0; i < 100; i++ {
		tableName := fmt.Sprintf("table_%d", i)
		_, err := tm.CreateTable(tableName, []byte("schema"), columns)
		require.NoError(t, err)
	}
	createDuration := time.Since(startTime)

	// List tables
	startTime = time.Now()
	tables := tm.ListTables()
	listDuration := time.Since(startTime)

	assert.Len(t, tables, 100)
	assert.Less(t, createDuration, 1*time.Second, "Creating 100 tables should be fast")
	assert.Less(t, listDuration, 100*time.Millisecond, "Listing tables should be very fast")
}

func TestEncodingManager_Performance(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping performance test in short mode")
	}

	em := createTestEncodingManager()

	// Create test data
	data := make(map[string]interface{})
	for i := 0; i < 1000; i++ {
		data[fmt.Sprintf("key_%d", i)] = fmt.Sprintf("value_%d", i)
	}

	// Test encoding performance
	startTime := time.Now()
	for i := 0; i < 100; i++ {
		_, err := em.Encode(data, EncodingBSATN, nil)
		require.NoError(t, err)
	}
	encodeDuration := time.Since(startTime)

	// Get statistics
	stats := em.GetStatistics()
	assert.Equal(t, uint64(100), stats.TotalEncoded)
	assert.Greater(t, stats.EncodingTime, time.Duration(0))
	assert.Less(t, encodeDuration, 5*time.Second, "Encoding should be reasonably fast")
}
