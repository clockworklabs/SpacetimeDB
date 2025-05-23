package schema

import (
	"fmt"
	"sync"
	"testing"
)

func TestTableRegistry_Basic(t *testing.T) {
	registry := NewTableRegistry()

	// Test empty registry
	if !registry.IsEmpty() {
		t.Error("new registry should be empty")
	}
	if registry.Count() != 0 {
		t.Error("new registry should have count 0")
	}

	// Create test table
	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
			{Name: "name", Type: TypeString},
		},
	}

	// Test registration
	err := registry.Register(table)
	if err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	// Test retrieval
	retrieved, exists := registry.GetTable("users")
	if !exists || retrieved.Name != "users" {
		t.Error("failed to retrieve registered table")
	}

	// Test count
	if registry.Count() != 1 {
		t.Errorf("expected count 1, got %d", registry.Count())
	}

	// Test existence
	if !registry.HasTable("users") {
		t.Error("registry should have users table")
	}
	if registry.HasTable("missing") {
		t.Error("registry should not have missing table")
	}
}

func TestTableRegistry_Registration(t *testing.T) {
	registry := NewTableRegistry()

	table1 := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	table2 := &TableInfo{
		Name: "posts",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	// Test successful registration
	if err := registry.Register(table1); err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	// Test duplicate registration (should fail)
	err := registry.Register(table1)
	if err == nil {
		t.Error("duplicate registration should fail")
	}

	// Test duplicate with overwrite allowed
	options := RegistrationOptions{AllowOverwrite: true}
	if err := registry.Register(table1, options); err != nil {
		t.Errorf("overwrite registration failed: %v", err)
	}

	// Test registration with validation disabled
	invalidTable := &TableInfo{
		Name:    "", // Invalid name
		Columns: []Column{},
	}

	noValidation := RegistrationOptions{ValidateSchema: false}
	if err := registry.Register(invalidTable, noValidation); err != nil {
		t.Errorf("registration with validation disabled should succeed: %v", err)
	}

	// Test registration with validation enabled (should fail)
	if err := registry.Register(invalidTable); err == nil {
		t.Error("registration of invalid table should fail")
	}

	// Test multiple table registration
	tables := []*TableInfo{table2}
	if err := registry.RegisterAll(tables); err != nil {
		t.Errorf("bulk registration failed: %v", err)
	}
}

func TestTableRegistry_IDManagement(t *testing.T) {
	registry := NewTableRegistry()

	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	// Test auto ID assignment
	if err := registry.Register(table); err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	if table.TableID == nil {
		t.Error("table ID should be assigned")
	}
	if *table.TableID != 1 {
		t.Errorf("expected table ID 1, got %d", *table.TableID)
	}

	// Test retrieval by ID
	retrieved, exists := registry.GetTableByID(*table.TableID)
	if !exists || retrieved.Name != "users" {
		t.Error("failed to retrieve table by ID")
	}

	// Test ID existence check
	if !registry.HasTableID(*table.TableID) {
		t.Error("registry should have table with assigned ID")
	}

	// Test manual ID update
	newID := TableID(100)
	if err := registry.UpdateTableID("users", newID); err != nil {
		t.Errorf("ID update failed: %v", err)
	}

	if *table.TableID != newID {
		t.Errorf("expected table ID %d, got %d", newID, *table.TableID)
	}

	// Test ID collision
	table2 := &TableInfo{
		Name: "posts",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	if err := registry.Register(table2); err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	// Try to assign same ID to different table
	if err := registry.UpdateTableID("posts", newID); err == nil {
		t.Error("ID collision should be prevented")
	}
}

func TestTableRegistry_Collections(t *testing.T) {
	registry := NewTableRegistry()

	// Register multiple tables
	tables := []*TableInfo{
		{
			Name: "users",
			Columns: []Column{
				{Name: "id", Type: TypeU32, PrimaryKey: true},
			},
		},
		{
			Name: "posts",
			Columns: []Column{
				{Name: "id", Type: TypeU32, PrimaryKey: true},
			},
		},
		{
			Name: "comments",
			Columns: []Column{
				{Name: "id", Type: TypeU32, PrimaryKey: true},
			},
		},
	}

	if err := registry.RegisterAll(tables); err != nil {
		t.Fatalf("bulk registration failed: %v", err)
	}

	// Test GetAllTables
	allTables := registry.GetAllTables()
	if len(allTables) != 3 {
		t.Errorf("expected 3 tables, got %d", len(allTables))
	}

	// Test GetTableNames
	names := registry.GetTableNames()
	if len(names) != 3 {
		t.Errorf("expected 3 names, got %d", len(names))
	}

	// Check names are sorted
	if names[0] != "comments" || names[1] != "posts" || names[2] != "users" {
		t.Errorf("names not sorted correctly: %v", names)
	}

	// Test GetTableIDs
	ids := registry.GetTableIDs()
	if len(ids) != 3 {
		t.Errorf("expected 3 IDs, got %d", len(ids))
	}

	// Check IDs are sorted
	for i := 1; i < len(ids); i++ {
		if ids[i-1] >= ids[i] {
			t.Errorf("IDs not sorted correctly: %v", ids)
		}
	}
}

func TestTableRegistry_Management(t *testing.T) {
	registry := NewTableRegistry()

	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	if err := registry.Register(table); err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	// Test removal by name
	if !registry.Remove("users") {
		t.Error("removal should succeed")
	}
	if registry.Remove("users") {
		t.Error("second removal should fail")
	}

	// Re-register for ID removal test
	if err := registry.Register(table); err != nil {
		t.Fatalf("re-registration failed: %v", err)
	}

	tableID := *table.TableID

	// Test removal by ID
	if !registry.RemoveByID(tableID) {
		t.Error("removal by ID should succeed")
	}
	if registry.RemoveByID(tableID) {
		t.Error("second removal by ID should fail")
	}

	// Test clear
	if err := registry.Register(table); err != nil {
		t.Fatalf("re-registration failed: %v", err)
	}

	registry.Clear()
	if !registry.IsEmpty() {
		t.Error("registry should be empty after clear")
	}
}

func TestTableRegistry_Validation(t *testing.T) {
	registry := NewTableRegistry()

	validTable := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	invalidTable := &TableInfo{
		Name: "posts",
		Columns: []Column{
			{Name: "", Type: TypeU32}, // Invalid column name
		},
	}

	// Register valid table
	if err := registry.Register(validTable); err != nil {
		t.Fatalf("valid table registration failed: %v", err)
	}

	// Register invalid table without validation
	noValidation := RegistrationOptions{ValidateSchema: false}
	if err := registry.Register(invalidTable, noValidation); err != nil {
		t.Fatalf("registration without validation failed: %v", err)
	}

	// Test ValidateAll (should fail due to invalid table)
	if err := registry.ValidateAll(); err == nil {
		t.Error("ValidateAll should fail with invalid table present")
	}

	// Test consistency check
	if err := registry.CheckConsistency(); err != nil {
		t.Errorf("consistency check failed: %v", err)
	}
}

func TestTableRegistry_MustGet(t *testing.T) {
	registry := NewTableRegistry()

	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	if err := registry.Register(table); err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	// Test successful MustGetTable
	retrieved := registry.MustGetTable("users")
	if retrieved.Name != "users" {
		t.Error("MustGetTable failed")
	}

	// Test MustGetTable panic
	defer func() {
		if r := recover(); r == nil {
			t.Error("MustGetTable should panic for missing table")
		}
	}()
	registry.MustGetTable("missing")
}

func TestTableRegistry_Stats(t *testing.T) {
	registry := NewTableRegistry()

	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
			{Name: "name", Type: TypeString},
		},
		Indexes: []Index{
			{Name: "idx_name", Type: IndexTypeBTree, Columns: []string{"name"}},
		},
	}

	if err := registry.Register(table); err != nil {
		t.Fatalf("registration failed: %v", err)
	}

	stats := registry.GetStats()
	if stats.TableCount != 1 {
		t.Errorf("expected table count 1, got %d", stats.TableCount)
	}
	if stats.ColumnCount != 2 {
		t.Errorf("expected column count 2, got %d", stats.ColumnCount)
	}
	if stats.IndexCount != 1 {
		t.Errorf("expected index count 1, got %d", stats.IndexCount)
	}
	if stats.TablesWithIDs != 1 {
		t.Errorf("expected tables with IDs 1, got %d", stats.TablesWithIDs)
	}

	// Test string representation
	str := registry.String()
	if str == "" {
		t.Error("string representation should not be empty")
	}
}

func TestTableRegistry_ThreadSafety(t *testing.T) {
	registry := NewTableRegistry()

	const numGoroutines = 10
	const numTables = 5

	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	// Concurrent registrations
	for i := 0; i < numGoroutines; i++ {
		go func(id int) {
			defer wg.Done()

			for j := 0; j < numTables; j++ {
				table := &TableInfo{
					Name: fmt.Sprintf("table_%d_%d", id, j),
					Columns: []Column{
						{Name: "id", Type: TypeU32, PrimaryKey: true},
					},
				}

				registry.Register(table)
			}
		}(i)
	}

	wg.Wait()

	// Verify all tables were registered
	expectedCount := numGoroutines * numTables
	if registry.Count() != expectedCount {
		t.Errorf("expected %d tables, got %d", expectedCount, registry.Count())
	}

	// Concurrent reads
	wg.Add(numGoroutines)
	for i := 0; i < numGoroutines; i++ {
		go func() {
			defer wg.Done()

			for j := 0; j < 100; j++ {
				registry.GetAllTables()
				registry.GetTableNames()
				registry.GetStats()
			}
		}()
	}

	wg.Wait()
}

func TestGlobalRegistry(t *testing.T) {
	// Clear global registry first
	GlobalClear()

	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	// Test global registration
	if err := GlobalRegister(table); err != nil {
		t.Fatalf("global registration failed: %v", err)
	}

	// Test global retrieval
	retrieved, exists := GlobalGetTable("users")
	if !exists || retrieved.Name != "users" {
		t.Error("global retrieval failed")
	}

	// Test global MustGet
	mustRetrieved := GlobalMustGetTable("users")
	if mustRetrieved.Name != "users" {
		t.Error("global MustGet failed")
	}

	// Test global existence check
	if !GlobalHasTable("users") {
		t.Error("global HasTable failed")
	}

	// Test global count
	if GlobalCount() != 1 {
		t.Errorf("expected global count 1, got %d", GlobalCount())
	}

	// Test global collection methods
	allTables := GlobalGetAllTables()
	if len(allTables) != 1 {
		t.Errorf("expected 1 global table, got %d", len(allTables))
	}

	names := GlobalGetTableNames()
	if len(names) != 1 || names[0] != "users" {
		t.Errorf("expected ['users'], got %v", names)
	}

	// Test global validation
	if err := GlobalValidateAll(); err != nil {
		t.Errorf("global validation failed: %v", err)
	}

	// Test global stats
	stats := GlobalGetStats()
	if stats.TableCount != 1 {
		t.Errorf("expected global table count 1, got %d", stats.TableCount)
	}

	// Test global clear
	GlobalClear()
	if GlobalCount() != 0 {
		t.Error("global registry should be empty after clear")
	}
}

func TestRegistrationOptions(t *testing.T) {
	// Test default options
	defaults := DefaultRegistrationOptions()
	if !defaults.ValidateSchema || defaults.AllowOverwrite || !defaults.AssignIDs {
		t.Errorf("unexpected default options: %+v", defaults)
	}

	registry := NewTableRegistry()

	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
		},
	}

	// Test registration with custom options
	customOptions := RegistrationOptions{
		ValidateSchema: false,
		AllowOverwrite: true,
		AssignIDs:      false,
	}

	if err := registry.Register(table, customOptions); err != nil {
		t.Fatalf("registration with custom options failed: %v", err)
	}

	// TableID should not be assigned
	if table.TableID != nil {
		t.Error("table ID should not be assigned when AssignIDs is false")
	}
}
