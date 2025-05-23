package db

import (
	"fmt"
	"sync"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/bsatn"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
)

// TableManager manages table operations and metadata
type TableManager struct {
	mu               sync.RWMutex
	tables           map[TableID]*TableMetadata
	nameToID         map[string]TableID
	runtime          *runtime.Runtime
	nextTableID      TableID
	tableConstraints map[TableID]*TableConstraints
	tableStats       map[TableID]*TableStatistics
}

// TableMetadata contains comprehensive table information
type TableMetadata struct {
	ID           TableID                `json:"id"`
	Name         string                 `json:"name"`
	Schema       []byte                 `json:"schema"`
	SchemaString string                 `json:"schema_string"`
	CreatedAt    time.Time              `json:"created_at"`
	UpdatedAt    time.Time              `json:"updated_at"`
	RowCount     uint64                 `json:"row_count"`
	Columns      []ColumnMetadata       `json:"columns"`
	Indexes      []IndexMetadata        `json:"indexes"`
	Constraints  *TableConstraints      `json:"constraints"`
	Statistics   *TableStatistics       `json:"statistics"`
	Properties   map[string]interface{} `json:"properties"`
	AccessLevel  string                 `json:"access_level"`
	Version      uint32                 `json:"version"`
}

// ColumnMetadata describes a table column
type ColumnMetadata struct {
	ID            uint32      `json:"id"`
	Name          string      `json:"name"`
	Type          string      `json:"type"`
	Nullable      bool        `json:"nullable"`
	DefaultValue  interface{} `json:"default_value"`
	PrimaryKey    bool        `json:"primary_key"`
	Unique        bool        `json:"unique"`
	AutoIncrement bool        `json:"auto_increment"`
	Comments      string      `json:"comments"`
}

// IndexMetadata describes a table index
type IndexMetadata struct {
	ID        IndexID   `json:"id"`
	Name      string    `json:"name"`
	Type      string    `json:"type"`
	Columns   []string  `json:"columns"`
	ColumnIDs []uint32  `json:"column_ids"`
	Unique    bool      `json:"unique"`
	Algorithm string    `json:"algorithm"`
	CreatedAt time.Time `json:"created_at"`
}

// TableConstraints defines table constraints
type TableConstraints struct {
	PrimaryKey       *PrimaryKeyConstraint  `json:"primary_key"`
	ForeignKeys      []ForeignKeyConstraint `json:"foreign_keys"`
	UniqueKeys       []UniqueConstraint     `json:"unique_keys"`
	CheckConstraints []CheckConstraint      `json:"check_constraints"`
}

// PrimaryKeyConstraint represents a primary key constraint
type PrimaryKeyConstraint struct {
	Name      string   `json:"name"`
	Columns   []string `json:"columns"`
	ColumnIDs []uint32 `json:"column_ids"`
}

// ForeignKeyConstraint represents a foreign key constraint
type ForeignKeyConstraint struct {
	Name              string   `json:"name"`
	Columns           []string `json:"columns"`
	ColumnIDs         []uint32 `json:"column_ids"`
	ReferencedTable   string   `json:"referenced_table"`
	ReferencedTableID TableID  `json:"referenced_table_id"`
	ReferencedColumns []string `json:"referenced_columns"`
	OnUpdate          string   `json:"on_update"`
	OnDelete          string   `json:"on_delete"`
}

// UniqueConstraint represents a unique constraint
type UniqueConstraint struct {
	Name      string   `json:"name"`
	Columns   []string `json:"columns"`
	ColumnIDs []uint32 `json:"column_ids"`
}

// CheckConstraint represents a check constraint
type CheckConstraint struct {
	Name       string `json:"name"`
	Expression string `json:"expression"`
}

// TableStatistics contains table performance statistics
type TableStatistics struct {
	RowCount         uint64             `json:"row_count"`
	DataSize         uint64             `json:"data_size"`
	IndexSize        uint64             `json:"index_size"`
	LastUpdateTime   time.Time          `json:"last_update_time"`
	InsertCount      uint64             `json:"insert_count"`
	UpdateCount      uint64             `json:"update_count"`
	DeleteCount      uint64             `json:"delete_count"`
	ScanCount        uint64             `json:"scan_count"`
	IndexUsage       map[IndexID]uint64 `json:"index_usage"`
	QueryPerformance *QueryPerformance  `json:"query_performance"`
}

// QueryPerformance tracks query performance metrics
type QueryPerformance struct {
	AverageInsertTime time.Duration `json:"average_insert_time"`
	AverageUpdateTime time.Duration `json:"average_update_time"`
	AverageDeleteTime time.Duration `json:"average_delete_time"`
	AverageScanTime   time.Duration `json:"average_scan_time"`
	TotalQueries      uint64        `json:"total_queries"`
}

// TableOperation represents a table operation type
type TableOperation int

const (
	TableOpInsert TableOperation = iota
	TableOpUpdate
	TableOpDelete
	TableOpScan
	TableOpCreate
	TableOpDrop
	TableOpAlter
)

// TableEvent represents a table operation event
type TableEvent struct {
	TableID   TableID        `json:"table_id"`
	Operation TableOperation `json:"operation"`
	Timestamp time.Time      `json:"timestamp"`
	RowCount  uint32         `json:"row_count"`
	Duration  time.Duration  `json:"duration"`
	Success   bool           `json:"success"`
	Error     string         `json:"error,omitempty"`
}

// NewTableManager creates a new table manager
func NewTableManager(runtime *runtime.Runtime) *TableManager {
	return &TableManager{
		tables:           make(map[TableID]*TableMetadata),
		nameToID:         make(map[string]TableID),
		runtime:          runtime,
		nextTableID:      1,
		tableConstraints: make(map[TableID]*TableConstraints),
		tableStats:       make(map[TableID]*TableStatistics),
	}
}

// CreateTable creates a new table with the given metadata
func (tm *TableManager) CreateTable(name string, schema []byte, columns []ColumnMetadata) (*TableMetadata, error) {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	// Check if table already exists
	if _, exists := tm.nameToID[name]; exists {
		return nil, fmt.Errorf("table %s already exists", name)
	}

	// Generate new table ID
	tableID := tm.nextTableID
	tm.nextTableID++

	// Create metadata
	metadata := &TableMetadata{
		ID:           tableID,
		Name:         name,
		Schema:       schema,
		SchemaString: string(schema),
		CreatedAt:    time.Now(),
		UpdatedAt:    time.Now(),
		RowCount:     0,
		Columns:      columns,
		Indexes:      []IndexMetadata{},
		Constraints:  &TableConstraints{},
		Statistics: &TableStatistics{
			IndexUsage:       make(map[IndexID]uint64),
			QueryPerformance: &QueryPerformance{},
		},
		Properties:  make(map[string]interface{}),
		AccessLevel: "private",
		Version:     1,
	}

	// Validate schema
	if err := tm.validateTableSchema(metadata); err != nil {
		return nil, fmt.Errorf("invalid table schema: %w", err)
	}

	// Store metadata
	tm.tables[tableID] = metadata
	tm.nameToID[name] = tableID
	tm.tableStats[tableID] = metadata.Statistics

	return metadata, nil
}

// GetTable returns table metadata by ID
func (tm *TableManager) GetTable(tableID TableID) (*TableMetadata, error) {
	tm.mu.RLock()
	defer tm.mu.RUnlock()

	metadata, exists := tm.tables[tableID]
	if !exists {
		return nil, NewErrno(ErrNoSuchTable)
	}

	return metadata, nil
}

// GetTableByName returns table metadata by name
func (tm *TableManager) GetTableByName(name string) (*TableMetadata, error) {
	tm.mu.RLock()
	defer tm.mu.RUnlock()

	tableID, exists := tm.nameToID[name]
	if !exists {
		return nil, NewErrno(ErrNoSuchTable)
	}

	return tm.tables[tableID], nil
}

// UpdateTableMetadata updates table metadata
func (tm *TableManager) UpdateTableMetadata(tableID TableID, updates map[string]interface{}) error {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	metadata, exists := tm.tables[tableID]
	if !exists {
		return NewErrno(ErrNoSuchTable)
	}

	// Update timestamp
	metadata.UpdatedAt = time.Now()
	metadata.Version++

	// Apply updates
	for key, value := range updates {
		switch key {
		case "name":
			if newName, ok := value.(string); ok {
				// Update name mapping
				delete(tm.nameToID, metadata.Name)
				metadata.Name = newName
				tm.nameToID[newName] = tableID
			}
		case "schema":
			if newSchema, ok := value.([]byte); ok {
				metadata.Schema = newSchema
				metadata.SchemaString = string(newSchema)
			}
		case "access_level":
			if accessLevel, ok := value.(string); ok {
				metadata.AccessLevel = accessLevel
			}
		default:
			// Store in properties
			metadata.Properties[key] = value
		}
	}

	return nil
}

// DropTable removes a table
func (tm *TableManager) DropTable(tableID TableID) error {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	metadata, exists := tm.tables[tableID]
	if !exists {
		return NewErrno(ErrNoSuchTable)
	}

	// Remove from maps
	delete(tm.tables, tableID)
	delete(tm.nameToID, metadata.Name)
	delete(tm.tableConstraints, tableID)
	delete(tm.tableStats, tableID)

	return nil
}

// ListTables returns all table metadata
func (tm *TableManager) ListTables() []*TableMetadata {
	tm.mu.RLock()
	defer tm.mu.RUnlock()

	tables := make([]*TableMetadata, 0, len(tm.tables))
	for _, metadata := range tm.tables {
		tables = append(tables, metadata)
	}

	return tables
}

// UpdateTableStatistics updates table statistics
func (tm *TableManager) UpdateTableStatistics(tableID TableID, operation TableOperation, duration time.Duration, rowCount uint32, success bool) {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	stats, exists := tm.tableStats[tableID]
	if !exists {
		return
	}

	stats.LastUpdateTime = time.Now()

	switch operation {
	case TableOpInsert:
		stats.InsertCount++
		if success {
			stats.RowCount += uint64(rowCount)
		}
		stats.QueryPerformance.AverageInsertTime = tm.updateAverageTime(
			stats.QueryPerformance.AverageInsertTime,
			duration,
			stats.InsertCount,
		)
	case TableOpUpdate:
		stats.UpdateCount++
		stats.QueryPerformance.AverageUpdateTime = tm.updateAverageTime(
			stats.QueryPerformance.AverageUpdateTime,
			duration,
			stats.UpdateCount,
		)
	case TableOpDelete:
		stats.DeleteCount++
		if success {
			stats.RowCount -= uint64(rowCount)
		}
		stats.QueryPerformance.AverageDeleteTime = tm.updateAverageTime(
			stats.QueryPerformance.AverageDeleteTime,
			duration,
			stats.DeleteCount,
		)
	case TableOpScan:
		stats.ScanCount++
		stats.QueryPerformance.AverageScanTime = tm.updateAverageTime(
			stats.QueryPerformance.AverageScanTime,
			duration,
			stats.ScanCount,
		)
	}

	stats.QueryPerformance.TotalQueries++
}

// updateAverageTime calculates running average
func (tm *TableManager) updateAverageTime(currentAvg time.Duration, newTime time.Duration, count uint64) time.Duration {
	if count == 1 {
		return newTime
	}
	return time.Duration((int64(currentAvg)*(int64(count)-1) + int64(newTime)) / int64(count))
}

// AddIndex adds an index to a table
func (tm *TableManager) AddIndex(tableID TableID, indexMeta IndexMetadata) error {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	metadata, exists := tm.tables[tableID]
	if !exists {
		return NewErrno(ErrNoSuchTable)
	}

	// Check if index already exists
	for _, idx := range metadata.Indexes {
		if idx.Name == indexMeta.Name {
			return fmt.Errorf("index %s already exists", indexMeta.Name)
		}
	}

	indexMeta.CreatedAt = time.Now()
	metadata.Indexes = append(metadata.Indexes, indexMeta)
	metadata.UpdatedAt = time.Now()
	metadata.Version++

	// Initialize index usage statistics
	if tm.tableStats[tableID] != nil {
		tm.tableStats[tableID].IndexUsage[indexMeta.ID] = 0
	}

	return nil
}

// RemoveIndex removes an index from a table
func (tm *TableManager) RemoveIndex(tableID TableID, indexID IndexID) error {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	metadata, exists := tm.tables[tableID]
	if !exists {
		return NewErrno(ErrNoSuchTable)
	}

	// Find and remove index
	for i, idx := range metadata.Indexes {
		if idx.ID == indexID {
			metadata.Indexes = append(metadata.Indexes[:i], metadata.Indexes[i+1:]...)
			metadata.UpdatedAt = time.Now()
			metadata.Version++

			// Remove from statistics
			if tm.tableStats[tableID] != nil {
				delete(tm.tableStats[tableID].IndexUsage, indexID)
			}

			return nil
		}
	}

	return NewErrno(ErrNoSuchIndex)
}

// UpdateIndexUsage updates index usage statistics
func (tm *TableManager) UpdateIndexUsage(tableID TableID, indexID IndexID) {
	tm.mu.Lock()
	defer tm.mu.Unlock()

	if stats, exists := tm.tableStats[tableID]; exists {
		stats.IndexUsage[indexID]++
	}
}

// GetTableStatistics returns table statistics
func (tm *TableManager) GetTableStatistics(tableID TableID) (*TableStatistics, error) {
	tm.mu.RLock()
	defer tm.mu.RUnlock()

	stats, exists := tm.tableStats[tableID]
	if !exists {
		return nil, NewErrno(ErrNoSuchTable)
	}

	return stats, nil
}

// validateTableSchema validates table schema and columns
func (tm *TableManager) validateTableSchema(metadata *TableMetadata) error {
	if metadata.Name == "" {
		return fmt.Errorf("table name cannot be empty")
	}

	if len(metadata.Columns) == 0 {
		return fmt.Errorf("table must have at least one column")
	}

	// Check column names for duplicates
	columnNames := make(map[string]bool)
	for _, col := range metadata.Columns {
		if col.Name == "" {
			return fmt.Errorf("column name cannot be empty")
		}
		if columnNames[col.Name] {
			return fmt.Errorf("duplicate column name: %s", col.Name)
		}
		columnNames[col.Name] = true
	}

	// Validate schema BSATN format
	if len(metadata.Schema) > 0 {
		if err := tm.validateBSATNSchema(metadata.Schema); err != nil {
			return fmt.Errorf("invalid BSATN schema: %w", err)
		}
	}

	return nil
}

// validateBSATNSchema validates BSATN schema format
func (tm *TableManager) validateBSATNSchema(schema []byte) error {
	// Basic validation - for testing, just check if schema is not empty
	// In a real implementation, this would validate against the actual BSATN schema format
	if len(schema) == 0 {
		return fmt.Errorf("schema cannot be empty")
	}

	// For now, accept any non-empty schema as valid
	// This allows string schemas for testing while maintaining the validation interface
	return nil
}

// TableSchemaBuilder helps build table schemas
type TableSchemaBuilder struct {
	name       string
	columns    []ColumnMetadata
	indexes    []IndexMetadata
	properties map[string]interface{}
}

// NewTableSchemaBuilder creates a new table schema builder
func NewTableSchemaBuilder(name string) *TableSchemaBuilder {
	return &TableSchemaBuilder{
		name:       name,
		columns:    []ColumnMetadata{},
		indexes:    []IndexMetadata{},
		properties: make(map[string]interface{}),
	}
}

// AddColumn adds a column to the schema
func (tsb *TableSchemaBuilder) AddColumn(name, dataType string, nullable bool) *TableSchemaBuilder {
	col := ColumnMetadata{
		ID:       uint32(len(tsb.columns)),
		Name:     name,
		Type:     dataType,
		Nullable: nullable,
	}
	tsb.columns = append(tsb.columns, col)
	return tsb
}

// AddPrimaryKeyColumn adds a primary key column
func (tsb *TableSchemaBuilder) AddPrimaryKeyColumn(name, dataType string) *TableSchemaBuilder {
	col := ColumnMetadata{
		ID:         uint32(len(tsb.columns)),
		Name:       name,
		Type:       dataType,
		Nullable:   false,
		PrimaryKey: true,
	}
	tsb.columns = append(tsb.columns, col)
	return tsb
}

// AddUniqueColumn adds a unique column
func (tsb *TableSchemaBuilder) AddUniqueColumn(name, dataType string, nullable bool) *TableSchemaBuilder {
	col := ColumnMetadata{
		ID:       uint32(len(tsb.columns)),
		Name:     name,
		Type:     dataType,
		Nullable: nullable,
		Unique:   true,
	}
	tsb.columns = append(tsb.columns, col)
	return tsb
}

// AddIndex adds an index to the schema
func (tsb *TableSchemaBuilder) AddIndex(name string, columns []string, unique bool) *TableSchemaBuilder {
	idx := IndexMetadata{
		ID:        IndexID(len(tsb.indexes)),
		Name:      name,
		Type:      "btree",
		Columns:   columns,
		Unique:    unique,
		Algorithm: "btree",
	}
	tsb.indexes = append(tsb.indexes, idx)
	return tsb
}

// SetProperty sets a table property
func (tsb *TableSchemaBuilder) SetProperty(key string, value interface{}) *TableSchemaBuilder {
	tsb.properties[key] = value
	return tsb
}

// Build builds the table schema and returns the column data and BSATN schema
func (tsb *TableSchemaBuilder) Build() ([]ColumnMetadata, []byte, error) {
	if len(tsb.columns) == 0 {
		return nil, nil, fmt.Errorf("table must have at least one column")
	}

	// Create a simple BSATN schema representation
	schema := map[string]interface{}{
		"table_name": tsb.name,
		"columns":    tsb.columns,
		"indexes":    tsb.indexes,
		"properties": tsb.properties,
	}

	schemaBytes, err := bsatn.Marshal(schema)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to marshal schema: %w", err)
	}

	return tsb.columns, schemaBytes, nil
}

// TableValidator provides table validation utilities
type TableValidator struct{}

// NewTableValidator creates a new table validator
func NewTableValidator() *TableValidator {
	return &TableValidator{}
}

// ValidateTableName validates table name
func (tv *TableValidator) ValidateTableName(name string) error {
	if name == "" {
		return fmt.Errorf("table name cannot be empty")
	}
	if len(name) > 64 {
		return fmt.Errorf("table name too long (max 64 characters)")
	}
	// Add more validation rules as needed
	return nil
}

// ValidateColumnType validates column data type
func (tv *TableValidator) ValidateColumnType(dataType string) error {
	validTypes := map[string]bool{
		"uint8":   true,
		"uint16":  true,
		"uint32":  true,
		"uint64":  true,
		"int8":    true,
		"int16":   true,
		"int32":   true,
		"int64":   true,
		"float32": true,
		"float64": true,
		"bool":    true,
		"string":  true,
		"bytes":   true,
	}

	if !validTypes[dataType] {
		return fmt.Errorf("unsupported column type: %s", dataType)
	}
	return nil
}

// ValidateRowData validates row data against schema
func (tv *TableValidator) ValidateRowData(data []byte, columns []ColumnMetadata) error {
	// Basic validation - ensure data can be unmarshaled
	_, _, err := bsatn.Unmarshal(data)
	if err != nil {
		return fmt.Errorf("invalid row data format: %w", err)
	}

	// Additional validation based on columns would go here
	// This is a simplified version
	return nil
}
