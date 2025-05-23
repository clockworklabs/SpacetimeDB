package database

import (
	"fmt"
	"reflect"
	"sync"
)

// SimpleTableAccessor provides high-level, type-safe access to SpacetimeDB tables
// This is a working foundation for Phase 4 without external dependencies
type SimpleTableAccessor[T any] struct {
	name string
	mu   sync.RWMutex

	// Cached reflection info for performance
	entityType reflect.Type
	pkField    *reflect.StructField
	pkIndex    int

	// Simple in-memory storage for demo purposes
	data map[interface{}]T
}

// SimpleTableError represents errors from table operations
type SimpleTableError struct {
	Operation string
	Table     string
	Reason    string
	Err       error
}

func (e *SimpleTableError) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("table %s %s failed: %s: %v", e.Table, e.Operation, e.Reason, e.Err)
	}
	return fmt.Sprintf("table %s %s failed: %s", e.Table, e.Operation, e.Reason)
}

func (e *SimpleTableError) Unwrap() error {
	return e.Err
}

// NewSimpleTable creates a new type-safe table accessor
func NewSimpleTable[T any](name string) (*SimpleTableAccessor[T], error) {
	// Get type information
	var zero T
	entityType := reflect.TypeOf(zero)
	if entityType.Kind() == reflect.Ptr {
		entityType = entityType.Elem()
	}

	if entityType.Kind() != reflect.Struct {
		return nil, &SimpleTableError{
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
		return nil, &SimpleTableError{
			Operation: "create",
			Table:     name,
			Reason:    "no primary key field found (use `spacetime:\"primary_key\"` tag or name field 'ID')",
		}
	}

	return &SimpleTableAccessor[T]{
		name:       name,
		entityType: entityType,
		pkField:    pkField,
		pkIndex:    pkIndex,
		data:       make(map[interface{}]T),
	}, nil
}

// Core CRUD Operations

// Insert adds a new entity to the table
func (t *SimpleTableAccessor[T]) Insert(entity T) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	// Get primary key value using reflection
	entityValue := reflect.ValueOf(entity)
	if entityValue.Kind() == reflect.Ptr {
		entityValue = entityValue.Elem()
	}

	pkValue := entityValue.Field(t.pkIndex)
	pkInterface := pkValue.Interface()

	// Check if entity already exists
	if _, exists := t.data[pkInterface]; exists {
		return &SimpleTableError{
			Operation: "insert",
			Table:     t.name,
			Reason:    fmt.Sprintf("entity with primary key %v already exists", pkInterface),
		}
	}

	// Store entity
	t.data[pkInterface] = entity

	return nil
}

// FindByID retrieves an entity by its primary key
func (t *SimpleTableAccessor[T]) FindByID(id interface{}) (T, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	var zero T

	// Validate ID type matches primary key
	if err := t.validatePrimaryKey(id); err != nil {
		return zero, &SimpleTableError{
			Operation: "find_by_id",
			Table:     t.name,
			Reason:    "invalid primary key",
			Err:       err,
		}
	}

	// Retrieve entity
	entity, exists := t.data[id]
	if !exists {
		return zero, &SimpleTableError{
			Operation: "find_by_id",
			Table:     t.name,
			Reason:    fmt.Sprintf("entity with ID %v not found", id),
		}
	}

	return entity, nil
}

// Update modifies an existing entity
func (t *SimpleTableAccessor[T]) Update(id interface{}, entity T) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	// Validate ID and entity
	if err := t.validatePrimaryKey(id); err != nil {
		return &SimpleTableError{
			Operation: "update",
			Table:     t.name,
			Reason:    "invalid primary key",
			Err:       err,
		}
	}

	// Check if entity exists
	if _, exists := t.data[id]; !exists {
		return &SimpleTableError{
			Operation: "update",
			Table:     t.name,
			Reason:    fmt.Sprintf("entity with ID %v not found", id),
		}
	}

	// Update entity
	t.data[id] = entity

	return nil
}

// Delete removes an entity by ID
func (t *SimpleTableAccessor[T]) Delete(id interface{}) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if err := t.validatePrimaryKey(id); err != nil {
		return &SimpleTableError{
			Operation: "delete",
			Table:     t.name,
			Reason:    "invalid primary key",
			Err:       err,
		}
	}

	// Check if entity exists
	if _, exists := t.data[id]; !exists {
		return &SimpleTableError{
			Operation: "delete",
			Table:     t.name,
			Reason:    fmt.Sprintf("entity with ID %v not found", id),
		}
	}

	// Delete entity
	delete(t.data, id)

	return nil
}

// FindAll retrieves all entities in the table
func (t *SimpleTableAccessor[T]) FindAll() ([]T, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	entities := make([]T, 0, len(t.data))
	for _, entity := range t.data {
		entities = append(entities, entity)
	}

	return entities, nil
}

// FindWhere retrieves entities matching a predicate function
func (t *SimpleTableAccessor[T]) FindWhere(predicate func(T) bool) ([]T, error) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	var entities []T
	for _, entity := range t.data {
		if predicate(entity) {
			entities = append(entities, entity)
		}
	}

	return entities, nil
}

// InsertBatch efficiently inserts multiple entities
func (t *SimpleTableAccessor[T]) InsertBatch(entities []T) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if len(entities) == 0 {
		return nil
	}

	// Validate all entities first and collect their keys
	entityKeys := make([]interface{}, len(entities))
	for i, entity := range entities {
		entityValue := reflect.ValueOf(entity)
		if entityValue.Kind() == reflect.Ptr {
			entityValue = entityValue.Elem()
		}

		pkValue := entityValue.Field(t.pkIndex)
		pkInterface := pkValue.Interface()
		entityKeys[i] = pkInterface

		// Check for duplicates in batch
		for j := 0; j < i; j++ {
			if entityKeys[j] == pkInterface {
				return &SimpleTableError{
					Operation: "insert_batch",
					Table:     t.name,
					Reason:    fmt.Sprintf("duplicate primary key %v at positions %d and %d", pkInterface, j, i),
				}
			}
		}

		// Check if entity already exists in table
		if _, exists := t.data[pkInterface]; exists {
			return &SimpleTableError{
				Operation: "insert_batch",
				Table:     t.name,
				Reason:    fmt.Sprintf("entity with primary key %v already exists (entity %d)", pkInterface, i),
			}
		}
	}

	// Insert all entities
	for i, entity := range entities {
		t.data[entityKeys[i]] = entity
	}

	return nil
}

// validatePrimaryKey validates primary key value
func (t *SimpleTableAccessor[T]) validatePrimaryKey(id interface{}) error {
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
func (t *SimpleTableAccessor[T]) Name() string {
	return t.name
}

// EntityType returns the Go type for entities in this table
func (t *SimpleTableAccessor[T]) EntityType() reflect.Type {
	return t.entityType
}

// Count returns the number of entities in the table
func (t *SimpleTableAccessor[T]) Count() int {
	t.mu.RLock()
	defer t.mu.RUnlock()
	return len(t.data)
}

// IsEmpty returns true if the table has no entities
func (t *SimpleTableAccessor[T]) IsEmpty() bool {
	return t.Count() == 0
}

// Clear removes all entities from the table
func (t *SimpleTableAccessor[T]) Clear() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.data = make(map[interface{}]T)
}

// Stats returns table statistics
func (t *SimpleTableAccessor[T]) Stats() SimpleTableStats {
	t.mu.RLock()
	defer t.mu.RUnlock()

	return SimpleTableStats{
		TableName:      t.name,
		EntityType:     t.entityType.Name(),
		HasPrimaryKey:  t.pkField != nil,
		EntityCount:    len(t.data),
		PrimaryKeyType: t.pkField.Type.Name(),
	}
}

// SimpleTableStats provides table metrics and information
type SimpleTableStats struct {
	TableName      string
	EntityType     string
	HasPrimaryKey  bool
	EntityCount    int
	PrimaryKeyType string
}

// Global Simple Table Registry

var (
	globalSimpleTables = make(map[string]interface{})
	globalSimpleMutex  sync.RWMutex
)

// RegisterGlobalSimpleTable registers a table in the global registry
func RegisterGlobalSimpleTable[T any](name string, table *SimpleTableAccessor[T]) {
	globalSimpleMutex.Lock()
	defer globalSimpleMutex.Unlock()
	globalSimpleTables[name] = table
}

// GetGlobalSimpleTable retrieves a table from the global registry
func GetGlobalSimpleTable[T any](name string) (*SimpleTableAccessor[T], bool) {
	globalSimpleMutex.RLock()
	defer globalSimpleMutex.RUnlock()

	if table, exists := globalSimpleTables[name]; exists {
		if typedTable, ok := table.(*SimpleTableAccessor[T]); ok {
			return typedTable, true
		}
	}
	return nil, false
}

// GetSimpleTable creates or retrieves a type-safe table accessor
func GetSimpleTable[T any](name string) (*SimpleTableAccessor[T], error) {
	// Try to get from global registry first
	if table, exists := GetGlobalSimpleTable[T](name); exists {
		return table, nil
	}

	// Create new table
	table, err := NewSimpleTable[T](name)
	if err != nil {
		return nil, err
	}

	// Register globally for reuse
	RegisterGlobalSimpleTable(name, table)

	return table, nil
}

// MustGetSimpleTable creates a table accessor and panics on error (for initialization)
func MustGetSimpleTable[T any](name string) *SimpleTableAccessor[T] {
	table, err := GetSimpleTable[T](name)
	if err != nil {
		panic(fmt.Sprintf("failed to create table %s: %v", name, err))
	}
	return table
}
