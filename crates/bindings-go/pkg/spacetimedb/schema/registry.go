package schema

import (
	"fmt"
	"sort"
	"sync"
)

// SpacetimeDB Table Registry
// This provides centralized table definition management for Go games

// TableRegistry manages a collection of table definitions
// This is thread-safe and supports both name and ID-based lookups
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

// Registration Methods

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
		return fmt.Errorf("table '%s' already registered (use AllowOverwrite=true to replace)", table.Name)
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
	options := DefaultRegistrationOptions()
	if len(opts) > 0 {
		options = opts[0]
	}

	// Validate all tables first if requested
	if options.ValidateSchema {
		for i, table := range tables {
			if err := table.Validate(); err != nil {
				return fmt.Errorf("table %d validation failed: %w", i, err)
			}
		}
	}

	// Register all tables
	for i, table := range tables {
		if err := tr.Register(table, options); err != nil {
			return fmt.Errorf("failed to register table %d (%s): %w", i, table.Name, err)
		}
	}

	return nil
}

// Lookup Methods

// GetTable returns a table by name
func (tr *TableRegistry) GetTable(name string) (*TableInfo, bool) {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	table, exists := tr.tables[name]
	return table, exists
}

// GetTableByID returns a table by its ID
func (tr *TableRegistry) GetTableByID(id TableID) (*TableInfo, bool) {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	table, exists := tr.tablesByID[id]
	return table, exists
}

// MustGetTable returns a table by name, panics if not found
func (tr *TableRegistry) MustGetTable(name string) *TableInfo {
	table, exists := tr.GetTable(name)
	if !exists {
		panic(fmt.Sprintf("table '%s' not found in registry", name))
	}
	return table
}

// HasTable returns true if a table with the given name exists
func (tr *TableRegistry) HasTable(name string) bool {
	_, exists := tr.GetTable(name)
	return exists
}

// HasTableID returns true if a table with the given ID exists
func (tr *TableRegistry) HasTableID(id TableID) bool {
	_, exists := tr.GetTableByID(id)
	return exists
}

// Collection Methods

// GetAllTables returns all registered tables
func (tr *TableRegistry) GetAllTables() []*TableInfo {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	tables := make([]*TableInfo, 0, len(tr.tables))
	for _, table := range tr.tables {
		tables = append(tables, table)
	}

	// Sort by name for consistent ordering
	sort.Slice(tables, func(i, j int) bool {
		return tables[i].Name < tables[j].Name
	})

	return tables
}

// GetTableNames returns all registered table names
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

// GetTableIDs returns all assigned table IDs
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

// Count returns the number of registered tables
func (tr *TableRegistry) Count() int {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	return len(tr.tables)
}

// IsEmpty returns true if no tables are registered
func (tr *TableRegistry) IsEmpty() bool {
	return tr.Count() == 0
}

// Management Methods

// Clear removes all tables from the registry
func (tr *TableRegistry) Clear() {
	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	tr.tables = make(map[string]*TableInfo)
	tr.tablesByID = make(map[TableID]*TableInfo)
	tr.idCounter = 1
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

// Update Methods

// UpdateTableID assigns or updates a table's ID
func (tr *TableRegistry) UpdateTableID(name string, id TableID) error {
	tr.mutex.Lock()
	defer tr.mutex.Unlock()

	table, exists := tr.tables[name]
	if !exists {
		return fmt.Errorf("table '%s' not found", name)
	}

	// Check if ID is already in use
	if existing, inUse := tr.tablesByID[id]; inUse && existing != table {
		return fmt.Errorf("table ID %d is already in use by table '%s'", id, existing.Name)
	}

	// Remove old ID mapping if exists
	if table.TableID != nil {
		delete(tr.tablesByID, *table.TableID)
	}

	// Update ID
	table.TableID = &id
	tr.tablesByID[id] = table

	// Update counter if necessary
	if id >= tr.idCounter {
		tr.idCounter = id + 1
	}

	return nil
}

// Validation Methods

// ValidateAll validates all registered tables
func (tr *TableRegistry) ValidateAll() error {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	for name, table := range tr.tables {
		if err := table.Validate(); err != nil {
			return fmt.Errorf("table '%s': %w", name, err)
		}
	}

	return nil
}

// CheckConsistency verifies internal consistency of the registry
func (tr *TableRegistry) CheckConsistency() error {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	// Check that all tables with IDs are in the ID map
	for name, table := range tr.tables {
		if table.TableID != nil {
			if idTable, exists := tr.tablesByID[*table.TableID]; !exists {
				return fmt.Errorf("table '%s' has ID %d but is not in ID map", name, *table.TableID)
			} else if idTable != table {
				return fmt.Errorf("table '%s' ID %d points to different table instance", name, *table.TableID)
			}
		}
	}

	// Check that all ID-mapped tables are in the name map
	for id, table := range tr.tablesByID {
		if nameTable, exists := tr.tables[table.Name]; !exists {
			return fmt.Errorf("table ID %d ('%s') is not in name map", id, table.Name)
		} else if nameTable != table {
			return fmt.Errorf("table ID %d name '%s' points to different table instance", id, table.Name)
		}
	}

	return nil
}

// Information Methods

// GetStats returns registry statistics
func (tr *TableRegistry) GetStats() RegistryStats {
	tr.mutex.RLock()
	defer tr.mutex.RUnlock()

	stats := RegistryStats{
		TableCount:    len(tr.tables),
		TablesWithIDs: len(tr.tablesByID),
		NextTableID:   tr.idCounter,
		ColumnCount:   0,
		IndexCount:    0,
	}

	for _, table := range tr.tables {
		stats.ColumnCount += len(table.Columns)
		stats.IndexCount += len(table.Indexes)
	}

	return stats
}

// String returns a string representation of the registry
func (tr *TableRegistry) String() string {
	stats := tr.GetStats()
	return fmt.Sprintf("TableRegistry{tables=%d, columns=%d, indexes=%d}",
		stats.TableCount, stats.ColumnCount, stats.IndexCount)
}

// RegistryStats contains statistics about a table registry
type RegistryStats struct {
	TableCount    int     `json:"table_count"`
	TablesWithIDs int     `json:"tables_with_ids"`
	NextTableID   TableID `json:"next_table_id"`
	ColumnCount   int     `json:"column_count"`
	IndexCount    int     `json:"index_count"`
}

// Global Registry
// Most applications will use a single global registry

var globalRegistry = NewTableRegistry()

// Global Registry Functions

// GlobalRegister registers a table in the global registry
func GlobalRegister(table *TableInfo, opts ...RegistrationOptions) error {
	return globalRegistry.Register(table, opts...)
}

// GlobalRegisterAll registers multiple tables in the global registry
func GlobalRegisterAll(tables []*TableInfo, opts ...RegistrationOptions) error {
	return globalRegistry.RegisterAll(tables, opts...)
}

// GlobalGetTable returns a table from the global registry by name
func GlobalGetTable(name string) (*TableInfo, bool) {
	return globalRegistry.GetTable(name)
}

// GlobalGetTableByID returns a table from the global registry by ID
func GlobalGetTableByID(id TableID) (*TableInfo, bool) {
	return globalRegistry.GetTableByID(id)
}

// GlobalMustGetTable returns a table from the global registry, panics if not found
func GlobalMustGetTable(name string) *TableInfo {
	return globalRegistry.MustGetTable(name)
}

// GlobalHasTable returns true if a table exists in the global registry
func GlobalHasTable(name string) bool {
	return globalRegistry.HasTable(name)
}

// GlobalGetAllTables returns all tables from the global registry
func GlobalGetAllTables() []*TableInfo {
	return globalRegistry.GetAllTables()
}

// GlobalGetTableNames returns all table names from the global registry
func GlobalGetTableNames() []string {
	return globalRegistry.GetTableNames()
}

// GlobalCount returns the number of tables in the global registry
func GlobalCount() int {
	return globalRegistry.Count()
}

// GlobalClear clears the global registry
func GlobalClear() {
	globalRegistry.Clear()
}

// GlobalValidateAll validates all tables in the global registry
func GlobalValidateAll() error {
	return globalRegistry.ValidateAll()
}

// GlobalGetStats returns statistics for the global registry
func GlobalGetStats() RegistryStats {
	return globalRegistry.GetStats()
}

// GetGlobalRegistry returns the global table registry instance
func GetGlobalRegistry() *TableRegistry {
	return globalRegistry
}
