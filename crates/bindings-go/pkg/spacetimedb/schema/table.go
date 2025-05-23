package schema

import (
	"encoding/json"
	"fmt"
	"regexp"
	"strings"
)

// SpacetimeDB Table Schema Framework
// This provides the core schema definition functionality that all Go games need

// TableID represents a unique identifier for a SpacetimeDB table
type TableID uint32

// ColumnID represents a unique identifier for a table column
type ColumnID uint16

// IndexID represents a unique identifier for a table index
type IndexID uint32

// TableInfo contains metadata about a SpacetimeDB table
// This matches the table definition requirements of SpacetimeDB
type TableInfo struct {
	// Name is the table name, must be unique within a module
	Name string `json:"name"`

	// PublicRead indicates if the table is publicly readable
	PublicRead bool `json:"public_read"`

	// Columns defines the table columns
	Columns []Column `json:"columns"`

	// Indexes defines the table indexes
	Indexes []Index `json:"indexes"`

	// TableID is the unique identifier (set by SpacetimeDB)
	TableID *TableID `json:"table_id,omitempty"`
}

// Column represents a table column definition
// This matches SpacetimeDB's column schema requirements
type Column struct {
	// Name is the column name, must be unique within the table
	Name string `json:"name"`

	// Type is the SpacetimeDB type name (e.g., "u32", "string", "Identity")
	Type string `json:"type"`

	// PrimaryKey indicates if this column is the primary key
	PrimaryKey bool `json:"primary_key"`

	// AutoInc indicates if this column auto-increments
	AutoInc bool `json:"auto_inc"`

	// Unique indicates if this column has a unique constraint
	Unique bool `json:"unique"`

	// NotNull indicates if this column cannot be null
	NotNull bool `json:"not_null"`

	// DefaultValue is the default value (optional)
	DefaultValue string `json:"default_value,omitempty"`

	// Position is the column position in the table (0-based)
	Position ColumnID `json:"position"`
}

// Index represents a table index definition
// This matches SpacetimeDB's index schema requirements
type Index struct {
	// Name is the index name, must be unique within the table
	Name string `json:"name"`

	// Type is the index type ("btree", "hash", etc.)
	Type IndexType `json:"type"`

	// Columns are the indexed column names
	Columns []string `json:"columns"`

	// Unique indicates if this is a unique index
	Unique bool `json:"unique"`

	// Clustered indicates if this is a clustered index
	Clustered bool `json:"clustered"`

	// IndexID is the unique identifier (set by SpacetimeDB)
	IndexID *IndexID `json:"index_id,omitempty"`
}

// IndexType represents the type of index
type IndexType string

const (
	// IndexTypeBTree represents a B-tree index
	IndexTypeBTree IndexType = "btree"

	// IndexTypeHash represents a hash index
	IndexTypeHash IndexType = "hash"

	// IndexTypeDirect represents a direct index
	IndexTypeDirect IndexType = "direct"
)

// Common SpacetimeDB type names
const (
	TypeU8           = "u8"
	TypeU16          = "u16"
	TypeU32          = "u32"
	TypeU64          = "u64"
	TypeU128         = "u128"
	TypeU256         = "u256"
	TypeI8           = "i8"
	TypeI16          = "i16"
	TypeI32          = "i32"
	TypeI64          = "i64"
	TypeI128         = "i128"
	TypeI256         = "i256"
	TypeF32          = "f32"
	TypeF64          = "f64"
	TypeBool         = "bool"
	TypeString       = "string"
	TypeBytes        = "bytes"
	TypeIdentity     = "Identity"
	TypeTimestamp    = "Timestamp"
	TypeTimeDuration = "TimeDuration"
	TypeScheduleAt   = "ScheduleAt"
)

// Validation Methods

// Validate validates a TableInfo
func (t *TableInfo) Validate() error {
	if t.Name == "" {
		return fmt.Errorf("table name cannot be empty")
	}

	if !isValidIdentifier(t.Name) {
		return fmt.Errorf("table name '%s' is not a valid identifier", t.Name)
	}

	if len(t.Columns) == 0 {
		return fmt.Errorf("table must have at least one column")
	}

	// Validate column names are unique
	columnNames := make(map[string]bool)
	primaryKeyCount := 0

	for i, col := range t.Columns {
		if err := col.Validate(); err != nil {
			return fmt.Errorf("column %d: %w", i, err)
		}

		if columnNames[col.Name] {
			return fmt.Errorf("duplicate column name: %s", col.Name)
		}
		columnNames[col.Name] = true

		if col.PrimaryKey {
			primaryKeyCount++
		}
	}

	if primaryKeyCount > 1 {
		return fmt.Errorf("table can have at most one primary key column")
	}

	// Validate indexes
	indexNames := make(map[string]bool)
	for i, idx := range t.Indexes {
		if err := idx.Validate(columnNames); err != nil {
			return fmt.Errorf("index %d: %w", i, err)
		}

		if indexNames[idx.Name] {
			return fmt.Errorf("duplicate index name: %s", idx.Name)
		}
		indexNames[idx.Name] = true
	}

	return nil
}

// Validate validates a Column
func (c *Column) Validate() error {
	if c.Name == "" {
		return fmt.Errorf("column name cannot be empty")
	}

	if !isValidIdentifier(c.Name) {
		return fmt.Errorf("column name '%s' is not a valid identifier", c.Name)
	}

	if c.Type == "" {
		return fmt.Errorf("column type cannot be empty")
	}

	if !isValidType(c.Type) {
		return fmt.Errorf("column type '%s' is not a valid SpacetimeDB type", c.Type)
	}

	if c.AutoInc && !c.PrimaryKey {
		return fmt.Errorf("auto-increment columns must be primary keys")
	}

	if c.AutoInc && !isIntegerType(c.Type) {
		return fmt.Errorf("auto-increment columns must be integer types")
	}

	return nil
}

// Validate validates an Index
func (i *Index) Validate(availableColumns map[string]bool) error {
	if i.Name == "" {
		return fmt.Errorf("index name cannot be empty")
	}

	if !isValidIdentifier(i.Name) {
		return fmt.Errorf("index name '%s' is not a valid identifier", i.Name)
	}

	if len(i.Columns) == 0 {
		return fmt.Errorf("index must have at least one column")
	}

	if !isValidIndexType(i.Type) {
		return fmt.Errorf("index type '%s' is not valid", i.Type)
	}

	// Validate all columns exist
	for _, colName := range i.Columns {
		if !availableColumns[colName] {
			return fmt.Errorf("index column '%s' does not exist in table", colName)
		}
	}

	// Validate column uniqueness within index
	columnSet := make(map[string]bool)
	for _, colName := range i.Columns {
		if columnSet[colName] {
			return fmt.Errorf("duplicate column '%s' in index", colName)
		}
		columnSet[colName] = true
	}

	return nil
}

// Utility Methods

// GetPrimaryKeyColumn returns the primary key column, if any
func (t *TableInfo) GetPrimaryKeyColumn() *Column {
	for i := range t.Columns {
		if t.Columns[i].PrimaryKey {
			return &t.Columns[i]
		}
	}
	return nil
}

// GetColumn returns a column by name
func (t *TableInfo) GetColumn(name string) *Column {
	for i := range t.Columns {
		if t.Columns[i].Name == name {
			return &t.Columns[i]
		}
	}
	return nil
}

// GetIndex returns an index by name
func (t *TableInfo) GetIndex(name string) *Index {
	for i := range t.Indexes {
		if t.Indexes[i].Name == name {
			return &t.Indexes[i]
		}
	}
	return nil
}

// HasColumn returns true if the table has a column with the given name
func (t *TableInfo) HasColumn(name string) bool {
	return t.GetColumn(name) != nil
}

// HasIndex returns true if the table has an index with the given name
func (t *TableInfo) HasIndex(name string) bool {
	return t.GetIndex(name) != nil
}

// ColumnCount returns the number of columns
func (t *TableInfo) ColumnCount() int {
	return len(t.Columns)
}

// IndexCount returns the number of indexes
func (t *TableInfo) IndexCount() int {
	return len(t.Indexes)
}

// String returns a string representation of the table
func (t *TableInfo) String() string {
	return fmt.Sprintf("Table{name=%s, columns=%d, indexes=%d}",
		t.Name, len(t.Columns), len(t.Indexes))
}

// String returns a string representation of the column
func (c *Column) String() string {
	flags := []string{}
	if c.PrimaryKey {
		flags = append(flags, "PK")
	}
	if c.AutoInc {
		flags = append(flags, "AUTO")
	}
	if c.Unique {
		flags = append(flags, "UNIQUE")
	}
	if c.NotNull {
		flags = append(flags, "NOT NULL")
	}

	flagStr := ""
	if len(flags) > 0 {
		flagStr = " [" + strings.Join(flags, ",") + "]"
	}

	return fmt.Sprintf("Column{%s:%s%s}", c.Name, c.Type, flagStr)
}

// String returns a string representation of the index
func (i *Index) String() string {
	flags := []string{}
	if i.Unique {
		flags = append(flags, "UNIQUE")
	}
	if i.Clustered {
		flags = append(flags, "CLUSTERED")
	}

	flagStr := ""
	if len(flags) > 0 {
		flagStr = " [" + strings.Join(flags, ",") + "]"
	}

	return fmt.Sprintf("Index{%s:%s on (%s)%s}",
		i.Name, i.Type, strings.Join(i.Columns, ","), flagStr)
}

// Helper functions

var identifierRegex = regexp.MustCompile(`^[a-zA-Z_][a-zA-Z0-9_]*$`)

// isValidIdentifier checks if a string is a valid SpacetimeDB identifier
func isValidIdentifier(name string) bool {
	if len(name) == 0 || len(name) > 64 {
		return false
	}
	return identifierRegex.MatchString(name)
}

// isValidType checks if a string is a valid SpacetimeDB type
func isValidType(typeName string) bool {
	// Basic types
	basicTypes := map[string]bool{
		TypeU8: true, TypeU16: true, TypeU32: true, TypeU64: true, TypeU128: true, TypeU256: true,
		TypeI8: true, TypeI16: true, TypeI32: true, TypeI64: true, TypeI128: true, TypeI256: true,
		TypeF32: true, TypeF64: true, TypeBool: true, TypeString: true, TypeBytes: true,
		TypeIdentity: true, TypeTimestamp: true, TypeTimeDuration: true, TypeScheduleAt: true,
	}

	if basicTypes[typeName] {
		return true
	}

	// For now, only allow basic types in validation
	// Custom types could be added to a type registry in the future
	return false
}

// isIntegerType checks if a type is an integer type
func isIntegerType(typeName string) bool {
	intTypes := map[string]bool{
		TypeU8: true, TypeU16: true, TypeU32: true, TypeU64: true, TypeU128: true, TypeU256: true,
		TypeI8: true, TypeI16: true, TypeI32: true, TypeI64: true, TypeI128: true, TypeI256: true,
	}
	return intTypes[typeName]
}

// isValidIndexType checks if an index type is valid
func isValidIndexType(indexType IndexType) bool {
	switch indexType {
	case IndexTypeBTree, IndexTypeHash, IndexTypeDirect:
		return true
	default:
		return false
	}
}

// JSON serialization helpers

// MarshalJSON implements custom JSON encoding for TableInfo
func (t *TableInfo) MarshalJSON() ([]byte, error) {
	type Alias TableInfo
	return json.Marshal(&struct {
		*Alias
		ColumnCount int `json:"column_count"`
		IndexCount  int `json:"index_count"`
	}{
		Alias:       (*Alias)(t),
		ColumnCount: len(t.Columns),
		IndexCount:  len(t.Indexes),
	})
}

// Constructor functions

// NewTableInfo creates a new TableInfo with the given name
func NewTableInfo(name string) *TableInfo {
	return &TableInfo{
		Name:       name,
		PublicRead: true, // Default to public
		Columns:    make([]Column, 0),
		Indexes:    make([]Index, 0),
	}
}

// NewColumn creates a new Column with the given name and type
func NewColumn(name, typeName string) Column {
	return Column{
		Name: name,
		Type: typeName,
	}
}

// NewPrimaryKeyColumn creates a new primary key column
func NewPrimaryKeyColumn(name, typeName string) Column {
	return Column{
		Name:       name,
		Type:       typeName,
		PrimaryKey: true,
		NotNull:    true,
	}
}

// NewAutoIncColumn creates a new auto-increment primary key column
func NewAutoIncColumn(name, typeName string) Column {
	return Column{
		Name:       name,
		Type:       typeName,
		PrimaryKey: true,
		AutoInc:    true,
		NotNull:    true,
	}
}

// NewIndex creates a new index
func NewIndex(name string, indexType IndexType, columns []string) Index {
	return Index{
		Name:    name,
		Type:    indexType,
		Columns: columns,
	}
}

// NewBTreeIndex creates a new B-tree index
func NewBTreeIndex(name string, columns []string) Index {
	return NewIndex(name, IndexTypeBTree, columns)
}

// NewUniqueIndex creates a new unique index
func NewUniqueIndex(name string, indexType IndexType, columns []string) Index {
	idx := NewIndex(name, indexType, columns)
	idx.Unique = true
	return idx
}
