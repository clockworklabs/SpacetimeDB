package schema

import (
	"fmt"
	"sort"
	"sync"
)

// TableRegistry manages a collection of table definitions
type TableRegistry struct {
	// mutex protects concurrent access
	mutex sync.RWMutex

	// tables maps table names to their definitions
	tables map[string]*TableInfo

	// tablesByID maps table IDs to their definitions (when available)
	tablesByID map[TableID]*TableInfo

	// idCounter tracks the next available table ID for local registration
	idCounter TableID
}

// RegistrationOptions configures table registration behavior
type RegistrationOptions struct {
	// ValidateSchema enables schema validation during registration
	ValidateSchema bool

	// AllowOverwrite permits overwriting existing table definitions
	AllowOverwrite bool

	// AssignIDs automatically assigns table IDs during registration
	AssignIDs bool
}

// DefaultRegistrationOptions returns the default registration options
func DefaultRegistrationOptions() RegistrationOptions {
	return RegistrationOptions{
		ValidateSchema: true,
		AllowOverwrite: false,
		AssignIDs:      true,
	}
}

// NewTableRegistry creates a new table registry
func NewTableRegistry() *TableRegistry {
	return &TableRegistry{
		tables:     make(map[string]*TableInfo),
		tablesByID: make(map[TableID]*TableInfo),
		idCounter:  1, // Start from 1, 0 is reserved
	}
}

// IsEmpty returns true if the registry contains no tables
func (tr *TableRegistry) IsEmpty() bool {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()
	return len(tr.tables) == 0
}

// Count returns the number of tables in the registry
func (tr *TableRegistry) Count() int {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()
	return len(tr.tables)
}

// HasTable returns true if a table with the given name exists
func (tr *TableRegistry) HasTable(name string) bool {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()
	_, exists := tr.tables[name]
	return exists
}

// HasTableID returns true if a table with the given ID exists
func (tr *TableRegistry) HasTableID(id TableID) bool {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()
	_, exists := tr.tablesByID[id]
	return exists
}

// Register registers a table definition in the registry
func (tr *TableRegistry) Register(table *TableInfo, opts ...RegistrationOptions) error {
	if table == nil {
		return fmt.Errorf("table cannot be nil")
	}

	options := DefaultRegistrationOptions()
	if len(opts) > 0 {
		options = opts[0]
	}

	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	// Validate schema if requested
	if options.ValidateSchema {
		if err := table.Validate(); err != nil {
			return fmt.Errorf("table validation failed: %w", err)
		}
	}

	// Check for existing table
	if existing, exists := tr.tables[table.Name]; exists && !options.AllowOverwrite {
		return fmt.Errorf("table '%s' already registered", table.Name)
	} else if exists && existing.TableID != nil && table.TableID == nil {
		// Preserve existing ID if new table doesn't have one
		table.TableID = existing.TableID
	}

	// Assign ID if requested and none exists
	if options.AssignIDs && table.TableID == nil {
		newID := tr.idCounter
		table.TableID = &newID
		tr.idCounter++
	}

	// Assign column positions if not set
	for i := range table.Columns {
		table.Columns[i].Position = ColumnID(i)
	}

	// Register the table
	tr.tables[table.Name] = table

	// Register by ID if available
	if table.TableID != nil {
		tr.tablesByID[*table.TableID] = table
	}

	return nil
}

// RegisterAll registers multiple tables at once
func (tr *TableRegistry) RegisterAll(tables []*TableInfo, opts ...RegistrationOptions) error {
	for _, table := range tables {
		if err := tr.Register(table, opts...); err != nil {
			return err
		}
	}
	return nil
}

// GetTable returns a table by name
func (tr *TableRegistry) GetTable(name string) (*TableInfo, bool) {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	table, exists := tr.tables[name]
	return table, exists
}

// GetTableByID returns a table by ID
func (tr *TableRegistry) GetTableByID(id TableID) (*TableInfo, bool) {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	table, exists := tr.tablesByID[id]
	return table, exists
}

// MustGetTable returns a table by name or panics if not found
func (tr *TableRegistry) MustGetTable(name string) *TableInfo {
	table, exists := tr.GetTable(name)
	if !exists {
		panic(fmt.Sprintf("table '%s' not found", name))
	}
	return table
}

// UpdateTableID updates the ID of an existing table
func (tr *TableRegistry) UpdateTableID(name string, newID TableID) error {
	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	table, exists := tr.tables[name]
	if !exists {
		return fmt.Errorf("table '%s' not found", name)
	}

	// Check for ID collision
	if _, exists := tr.tablesByID[newID]; exists {
		return fmt.Errorf("table ID %d already in use", newID)
	}

	// Remove old ID mapping
	if table.TableID != nil {
		delete(tr.tablesByID, *table.TableID)
	}

	// Set new ID
	table.TableID = &newID
	tr.tablesByID[newID] = table

	return nil
}

// GetAllTables returns all tables in the registry
func (tr *TableRegistry) GetAllTables() []*TableInfo {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	tables := make([]*TableInfo, 0, len(tr.tables))
	for _, table := range tr.tables {
		tables = append(tables, table)
	}
	return tables
}

// GetTableNames returns all table names, sorted alphabetically
func (tr *TableRegistry) GetTableNames() []string {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	names := make([]string, 0, len(tr.tables))
	for name := range tr.tables {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// GetTableIDs returns all table IDs, sorted numerically
func (tr *TableRegistry) GetTableIDs() []TableID {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	ids := make([]TableID, 0, len(tr.tablesByID))
	for id := range tr.tablesByID {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool {
		return ids[i] < ids[j]
	})
	return ids
}

// Remove removes a table by name
func (tr *TableRegistry) Remove(name string) bool {
	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	table, exists := tr.tables[name]
	if !exists {
		return false
	}

	delete(tr.tables, name)
	if table.TableID != nil {
		delete(tr.tablesByID, *table.TableID)
	}
	return true
}

// RemoveByID removes a table by ID
func (tr *TableRegistry) RemoveByID(id TableID) bool {
	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	table, exists := tr.tablesByID[id]
	if !exists {
		return false
	}

	delete(tr.tablesByID, id)
	delete(tr.tables, table.Name)
	return true
}

// Clear removes all tables from the registry
func (tr *TableRegistry) Clear() {
	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	tr.tables = make(map[string]*TableInfo)
	tr.tablesByID = make(map[TableID]*TableInfo)
	tr.idCounter = 1
}

// ValidateAll validates all tables in the registry
func (tr *TableRegistry) ValidateAll() error {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	for name, table := range tr.tables {
		if err := table.Validate(); err != nil {
			return fmt.Errorf("table '%s' validation failed: %w", name, err)
		}
	}
	return nil
}

// CheckConsistency checks the internal consistency of the registry
func (tr *TableRegistry) CheckConsistency() error {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	// Check that all tables with IDs are in both maps
	for name, table := range tr.tables {
		if table.TableID != nil {
			if idTable, exists := tr.tablesByID[*table.TableID]; !exists || idTable != table {
				return fmt.Errorf("inconsistent ID mapping for table '%s'", name)
			}
		}
	}

	// Check reverse mapping
	for id, table := range tr.tablesByID {
		if nameTable, exists := tr.tables[table.Name]; !exists || nameTable != table {
			return fmt.Errorf("inconsistent name mapping for table ID %d", id)
		}
		if table.TableID == nil || *table.TableID != id {
			return fmt.Errorf("table ID mismatch for table '%s'", table.Name)
		}
	}

	return nil
}

// RegistryStats contains statistics about the registry
type RegistryStats struct {
	TableCount    int `json:"table_count"`
	ColumnCount   int `json:"column_count"`
	IndexCount    int `json:"index_count"`
	TablesWithIDs int `json:"tables_with_ids"`
}

// GetStats returns statistics about the registry
func (tr *TableRegistry) GetStats() RegistryStats {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	stats := RegistryStats{
		TableCount: len(tr.tables),
	}

	for _, table := range tr.tables {
		stats.ColumnCount += len(table.Columns)
		stats.IndexCount += len(table.Indexes)
		if table.TableID != nil {
			stats.TablesWithIDs++
		}
	}

	return stats
}

// String returns a string representation of the registry
func (tr *TableRegistry) String() string {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	return fmt.Sprintf("TableRegistry{tables: %d, tablesWithIDs: %d}",
		len(tr.tables), len(tr.tablesByID))
}

// Global Registry
var globalRegistry = NewTableRegistry()

// Global Registry Functions

// GlobalRegister registers a table in the global registry
func GlobalRegister(table *TableInfo, opts ...RegistrationOptions) error {
	return globalRegistry.Register(table, opts...)
}

// GlobalGetTable returns a table from the global registry by name
func GlobalGetTable(name string) (*TableInfo, bool) {
	return globalRegistry.GetTable(name)
}

// GlobalGetTableByID returns a table from the global registry by ID
func GlobalGetTableByID(id TableID) (*TableInfo, bool) {
	return globalRegistry.GetTableByID(id)
}

// GlobalMustGetTable returns a table from the global registry by name or panics
func GlobalMustGetTable(name string) *TableInfo {
	return globalRegistry.MustGetTable(name)
}

// GlobalHasTable returns true if a table exists in the global registry
func GlobalHasTable(name string) bool {
	return globalRegistry.HasTable(name)
}

// GlobalCount returns the number of tables in the global registry
func GlobalCount() int {
	return globalRegistry.Count()
}

// GlobalGetAllTables returns all tables from the global registry
func GlobalGetAllTables() []*TableInfo {
	return globalRegistry.GetAllTables()
}

// GlobalGetTableNames returns all table names from the global registry
func GlobalGetTableNames() []string {
	return globalRegistry.GetTableNames()
}

// GlobalValidateAll validates all tables in the global registry
func GlobalValidateAll() error {
	return globalRegistry.ValidateAll()
}

// GlobalGetStats returns statistics about the global registry
func GlobalGetStats() RegistryStats {
	return globalRegistry.GetStats()
}

// GlobalClear clears the global registry
func GlobalClear() {
	globalRegistry.Clear()
}

// GlobalRegisterAll registers multiple tables in the global registry
func GlobalRegisterAll(tables []*TableInfo, opts ...RegistrationOptions) error {
	return globalRegistry.RegisterAll(tables, opts...)
}

// GetGlobalRegistry returns the global registry instance
func GetGlobalRegistry() *TableRegistry {
	return globalRegistry
}
