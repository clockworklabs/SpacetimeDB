package database

import (
	"fmt"
	"io"
	"reflect"
	"sync"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/bsatn"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/schema"
)

// TableAccessor provides high-level, type-safe access to SpacetimeDB tables
type TableAccessor[T any] struct {
	name       string
	tableInfo  *schema.TableInfo
	serializer Serializer[T]
	mu         sync.RWMutex

	// Cached reflection info for performance
	entityType reflect.Type
	pkField    *reflect.StructField
	pkIndex    int
}

// Serializer defines how entities are serialized/deserialized
type Serializer[T any] interface {
	Serialize(entity T) ([]byte, error)
	Deserialize(data []byte) (T, error)
	SerializeBatch(entities []T) ([]byte, error)
	DeserializeBatch(data []byte) ([]T, error)
}

// BsatnSerializer provides high-performance BSATN serialization
type BsatnSerializer[T any] struct {
	encodeFunc func(io.Writer, T) error
	decodeFunc func(io.Reader) (T, error)
	sizeFunc   func(T) int
}

// Serialize converts an entity to bytes using BSATN
func (s *BsatnSerializer[T]) Serialize(entity T) ([]byte, error) {
	return bsatn.ToBytes(func(w io.Writer) error {
		return bsatn.EncodeStruct(w, entity)
	})
}

// Deserialize converts bytes back to an entity using BSATN
func (s *BsatnSerializer[T]) Deserialize(data []byte) (T, error) {
	var zero T
	// TODO: Implement BSATN struct decoding
	// For now, return zero value
	return zero, fmt.Errorf("BSATN struct decoding not implemented yet")
}

// SerializeBatch efficiently serializes multiple entities
func (s *BsatnSerializer[T]) SerializeBatch(entities []T) ([]byte, error) {
	return bsatn.ToBytes(func(w io.Writer) error {
		// Encode array length
		if err := bsatn.EncodeU32(w, uint32(len(entities))); err != nil {
			return err
		}
		// Encode each entity
		for _, entity := range entities {
			if err := bsatn.EncodeStruct(w, entity); err != nil {
				return err
			}
		}
		return nil
	})
}

// DeserializeBatch efficiently deserializes multiple entities
func (s *BsatnSerializer[T]) DeserializeBatch(data []byte) ([]T, error) {
	// TODO: Implement BSATN batch struct decoding
	return nil, fmt.Errorf("BSATN batch struct decoding not implemented yet")
}

// TableError represents errors from table operations
type TableError struct {
	Operation string
	Table     string
	Reason    string
	Err       error
}

func (e *TableError) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("table %s %s failed: %s: %v", e.Table, e.Operation, e.Reason, e.Err)
	}
	return fmt.Sprintf("table %s %s failed: %s", e.Table, e.Operation, e.Reason)
}

func (e *TableError) Unwrap() error {
	return e.Err
}

// TableOptions configures table behavior
type TableOptions struct {
	EnableCaching     bool
	CacheTTL          int64 // seconds
	BatchSize         int
	EnableMetrics     bool
	ValidationMode    ValidationMode
	SerializationMode SerializationMode
}

type ValidationMode int

const (
	ValidationOff ValidationMode = iota
	ValidationBasic
	ValidationStrict
)

type SerializationMode int

const (
	SerializationJSON SerializationMode = iota
	SerializationBSATN
	SerializationAuto
)

// DefaultTableOptions returns sensible defaults optimized for games
func DefaultTableOptions() *TableOptions {
	return &TableOptions{
		EnableCaching:     true,
		CacheTTL:          300, // 5 minutes
		BatchSize:         1000,
		EnableMetrics:     true,
		ValidationMode:    ValidationBasic,
		SerializationMode: SerializationBSATN, // Use our high-performance BSATN!
	}
}

// NewTableAccessor creates a new type-safe table accessor
func NewTableAccessor[T any](name string, options *TableOptions) (*TableAccessor[T], error) {
	if options == nil {
		options = DefaultTableOptions()
	}

	// Get type information
	var zero T
	entityType := reflect.TypeOf(zero)
	if entityType.Kind() == reflect.Ptr {
		entityType = entityType.Elem()
	}

	if entityType.Kind() != reflect.Struct {
		return nil, &TableError{
			Operation: "create",
			Table:     name,
			Reason:    "entity type must be a struct",
		}
	}

	// Find primary key field
	pkIndex := -1
	var pkField *reflect.StructField
	for i := 0; i < entityType.NumField(); i++ {
		field := entityType.Field(i)
		if tag := field.Tag.Get("spacetime"); tag == "primary_key" || tag == "pk" {
			pkIndex = i
			pkField = &field
			break
		}
		// Also check for common naming conventions
		if field.Name == "ID" || field.Name == "Id" {
			pkIndex = i
			pkField = &field
		}
	}

	if pkIndex == -1 {
		return nil, &TableError{
			Operation: "create",
			Table:     name,
			Reason:    "no primary key field found (use `spacetime:\"primary_key\"` tag or name field 'ID')",
		}
	}

	// Create serializer based on mode
	var serializer Serializer[T]
	switch options.SerializationMode {
	case SerializationBSATN:
		serializer = &BsatnSerializer[T]{
			// For now, use reflection-based encoding
			// TODO: Add code generation for optimal performance
		}
	default:
		return nil, &TableError{
			Operation: "create",
			Table:     name,
			Reason:    fmt.Sprintf("unsupported serialization mode: %v", options.SerializationMode),
		}
	}

	// Try to get table info from schema registry
	tableInfo, exists := schema.GlobalGetTable(name)
	if !exists {
		// Auto-create table info from struct (development mode)
		tableInfo = generateTableInfo(name, entityType)
		if err := schema.GlobalRegister(tableInfo); err != nil {
			return nil, &TableError{
				Operation: "create",
				Table:     name,
				Reason:    "failed to register auto-generated table schema",
				Err:       err,
			}
		}
	}

	return &TableAccessor[T]{
		name:       name,
		tableInfo:  tableInfo,
		serializer: serializer,
		entityType: entityType,
		pkField:    pkField,
		pkIndex:    pkIndex,
	}, nil
}

// generateTableInfo creates table schema from Go struct
func generateTableInfo(tableName string, entityType reflect.Type) *schema.TableInfo {
	table := schema.NewTableInfo(tableName)

	for i := 0; i < entityType.NumField(); i++ {
		field := entityType.Field(i)
		if !field.IsExported() {
			continue
		}

		// Map Go types to SpacetimeDB types
		columnType := mapGoTypeToSpaceTime(field.Type)
		if columnType == "" {
			continue // Skip unsupported types
		}

		// Check for primary key
		tag := field.Tag.Get("spacetime")
		if tag == "primary_key" || tag == "pk" || field.Name == "ID" || field.Name == "Id" {
			column := schema.NewPrimaryKeyColumn(field.Name, columnType)
			table.Columns = append(table.Columns, column)
		} else {
			column := schema.NewColumn(field.Name, columnType)
			table.Columns = append(table.Columns, column)
		}
	}

	return table
}

// mapGoTypeToSpaceTime maps Go types to SpacetimeDB schema types
func mapGoTypeToSpaceTime(t reflect.Type) string {
	switch t.Kind() {
	case reflect.Uint8:
		return schema.TypeU8
	case reflect.Uint16:
		return schema.TypeU16
	case reflect.Uint32:
		return schema.TypeU32
	case reflect.Uint64:
		return schema.TypeU64
	case reflect.Int8:
		return schema.TypeI8
	case reflect.Int16:
		return schema.TypeI16
	case reflect.Int32:
		return schema.TypeI32
	case reflect.Int64:
		return schema.TypeI64
	case reflect.Float32:
		return schema.TypeF32
	case reflect.Float64:
		return schema.TypeF64
	case reflect.Bool:
		return schema.TypeBool
	case reflect.String:
		return schema.TypeString
	case reflect.Slice:
		if t.Elem().Kind() == reflect.Uint8 {
			return schema.TypeBytes // []byte
		}
	}

	// Check for SpacetimeDB types
	typeName := t.Name()
	switch typeName {
	case "Identity":
		return schema.TypeIdentity
	case "Timestamp":
		return schema.TypeTimestamp
	case "TimeDuration":
		return schema.TypeTimeDuration
	case "ScheduleAt":
		return schema.TypeScheduleAt
	}

	return "" // Unsupported type
}

// Core CRUD Operations

// Insert adds a new entity to the table
func (t *TableAccessor[T]) Insert(entity T) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	// Validate entity
	if err := t.validateEntity(entity); err != nil {
		return &TableError{
			Operation: "insert",
			Table:     t.name,
			Reason:    "validation failed",
			Err:       err,
		}
	}

	// Serialize entity
	data, err := t.serializer.Serialize(entity)
	if err != nil {
		return &TableError{
			Operation: "insert",
			Table:     t.name,
			Reason:    "serialization failed",
			Err:       err,
		}
	}

	// TODO: Send to SpacetimeDB
	_ = data // For now, just validate the flow

	return nil
}

// FindByID retrieves an entity by its primary key
func (t *TableAccessor[T]) FindByID(id interface{}) (T, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	var zero T

	// Validate ID type matches primary key
	if err := t.validatePrimaryKey(id); err != nil {
		return zero, &TableError{
			Operation: "find_by_id",
			Table:     t.name,
			Reason:    "invalid primary key",
			Err:       err,
		}
	}

	// TODO: Query SpacetimeDB by ID
	// For now, return zero value
	return zero, &TableError{
		Operation: "find_by_id",
		Table:     t.name,
		Reason:    "not implemented yet",
	}
}

// Update modifies an existing entity
func (t *TableAccessor[T]) Update(id interface{}, entity T) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	// Validate ID and entity
	if err := t.validatePrimaryKey(id); err != nil {
		return &TableError{
			Operation: "update",
			Table:     t.name,
			Reason:    "invalid primary key",
			Err:       err,
		}
	}

	if err := t.validateEntity(entity); err != nil {
		return &TableError{
			Operation: "update",
			Table:     t.name,
			Reason:    "validation failed",
			Err:       err,
		}
	}

	// TODO: Update in SpacetimeDB
	return nil
}

// Delete removes an entity by ID
func (t *TableAccessor[T]) Delete(id interface{}) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if err := t.validatePrimaryKey(id); err != nil {
		return &TableError{
			Operation: "delete",
			Table:     t.name,
			Reason:    "invalid primary key",
			Err:       err,
		}
	}

	// TODO: Delete from SpacetimeDB
	return nil
}

// Batch Operations

// InsertBatch efficiently inserts multiple entities
func (t *TableAccessor[T]) InsertBatch(entities []T) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if len(entities) == 0 {
		return nil
	}

	// Validate all entities first
	for i, entity := range entities {
		if err := t.validateEntity(entity); err != nil {
			return &TableError{
				Operation: "insert_batch",
				Table:     t.name,
				Reason:    fmt.Sprintf("validation failed for entity %d", i),
				Err:       err,
			}
		}
	}

	// Serialize batch using high-performance BSATN
	data, err := t.serializer.SerializeBatch(entities)
	if err != nil {
		return &TableError{
			Operation: "insert_batch",
			Table:     t.name,
			Reason:    "batch serialization failed",
			Err:       err,
		}
	}

	// TODO: Send batch to SpacetimeDB
	_ = data

	return nil
}

// Validation

// validateEntity performs entity validation
func (t *TableAccessor[T]) validateEntity(entity T) error {
	// TODO: Add comprehensive validation
	// - Check required fields
	// - Validate field constraints
	// - Check foreign key relationships

	return nil
}

// validatePrimaryKey validates primary key value
func (t *TableAccessor[T]) validatePrimaryKey(id interface{}) error {
	if t.pkField == nil {
		return fmt.Errorf("no primary key defined")
	}

	// Check type compatibility
	idType := reflect.TypeOf(id)
	if !idType.AssignableTo(t.pkField.Type) {
		return fmt.Errorf("primary key type mismatch: expected %v, got %v",
			t.pkField.Type, idType)
	}

	return nil
}

// Utility Methods

// Name returns the table name
func (t *TableAccessor[T]) Name() string {
	return t.name
}

// Schema returns the table schema information
func (t *TableAccessor[T]) Schema() *schema.TableInfo {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return t.tableInfo
}

// EntityType returns the Go type for entities in this table
func (t *TableAccessor[T]) EntityType() reflect.Type {
	return t.entityType
}

// Stats returns table statistics
func (t *TableAccessor[T]) Stats() TableStats {
	t.mu.RLock()
	defer t.mu.RUnlock()

	return TableStats{
		TableName:     t.name,
		EntityType:    t.entityType.Name(),
		HasPrimaryKey: t.pkField != nil,
		ColumnCount:   len(t.tableInfo.Columns),
		IndexCount:    len(t.tableInfo.Indexes),
	}
}

// TableStats provides table metrics and information
type TableStats struct {
	TableName     string
	EntityType    string
	HasPrimaryKey bool
	ColumnCount   int
	IndexCount    int

	// TODO: Add runtime stats
	// TotalQueries   uint64
	// CacheHitRate   float64
	// AvgQueryTime   time.Duration
}

// Global Table Registry

var (
	globalTables = make(map[string]interface{})
	globalMutex  sync.RWMutex
)

// RegisterGlobalTable registers a table in the global registry
func RegisterGlobalTable[T any](name string, table *TableAccessor[T]) {
	globalMutex.Lock()
	defer globalMutex.Unlock()
	globalTables[name] = table
}

// GetGlobalTable retrieves a table from the global registry
func GetGlobalTable[T any](name string) (*TableAccessor[T], bool) {
	globalMutex.RLock()
	defer globalMutex.RUnlock()

	if table, exists := globalTables[name]; exists {
		if typedTable, ok := table.(*TableAccessor[T]); ok {
			return typedTable, true
		}
	}
	return nil, false
}

// ListGlobalTables returns all registered table names
func ListGlobalTables() []string {
	globalMutex.RLock()
	defer globalMutex.RUnlock()

	names := make([]string, 0, len(globalTables))
	for name := range globalTables {
		names = append(names, name)
	}
	return names
}

// Factory Functions

// GetTable creates or retrieves a type-safe table accessor
func GetTable[T any](name string) (*TableAccessor[T], error) {
	// Try to get from global registry first
	if table, exists := GetGlobalTable[T](name); exists {
		return table, nil
	}

	// Create new table with default options
	table, err := NewTableAccessor[T](name, DefaultTableOptions())
	if err != nil {
		return nil, err
	}

	// Register globally for reuse
	RegisterGlobalTable(name, table)

	return table, nil
}

// MustGetTable creates a table accessor and panics on error (for initialization)
func MustGetTable[T any](name string) *TableAccessor[T] {
	table, err := GetTable[T](name)
	if err != nil {
		panic(fmt.Sprintf("failed to create table %s: %v", name, err))
	}
	return table
}
