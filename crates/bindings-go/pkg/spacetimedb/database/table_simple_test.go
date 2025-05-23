package database

import (
	"testing"
)

// Test entity types
type SimplePlayer struct {
	ID     uint32 `spacetime:"primary_key"`
	Name   string
	Score  uint32
	Active bool
}

type SimpleGame struct {
	ID    uint32 `spacetime:"primary_key"`
	Name  string
	Round uint32
}

func TestSimpleTableCreation(t *testing.T) {
	// Test successful table creation
	t.Run("ValidEntity", func(t *testing.T) {
		table, err := NewSimpleTable[SimplePlayer]("players")
		if err != nil {
			t.Fatalf("Failed to create table: %v", err)
		}

		if table.Name() != "players" {
			t.Errorf("Expected table name 'players', got '%s'", table.Name())
		}

		stats := table.Stats()
		if stats.EntityType != "SimplePlayer" {
			t.Errorf("Expected entity type 'SimplePlayer', got '%s'", stats.EntityType)
		}

		if !stats.HasPrimaryKey {
			t.Error("Expected table to have primary key")
		}

		if stats.PrimaryKeyType != "uint32" {
			t.Errorf("Expected primary key type 'uint32', got '%s'", stats.PrimaryKeyType)
		}
	})

	// Test entity without struct
	t.Run("InvalidEntityType", func(t *testing.T) {
		_, err := NewSimpleTable[string]("invalid")
		if err == nil {
			t.Error("Expected error for non-struct entity type")
		}

		if tableErr, ok := err.(*SimpleTableError); ok {
			if tableErr.Operation != "create" {
				t.Errorf("Expected operation 'create', got '%s'", tableErr.Operation)
			}
		} else {
			t.Errorf("Expected SimpleTableError, got %T", err)
		}
	})
}

func TestSimpleTableCRUD(t *testing.T) {
	table, err := NewSimpleTable[SimplePlayer]("test_players")
	if err != nil {
		t.Fatalf("Failed to create table: %v", err)
	}

	// Test Insert
	t.Run("Insert", func(t *testing.T) {
		player := SimplePlayer{
			ID:     1,
			Name:   "Alice",
			Score:  1500,
			Active: true,
		}

		err := table.Insert(player)
		if err != nil {
			t.Errorf("Insert failed: %v", err)
		}

		// Verify count
		if table.Count() != 1 {
			t.Errorf("Expected count 1, got %d", table.Count())
		}

		// Test duplicate insert
		err = table.Insert(player)
		if err == nil {
			t.Error("Expected error for duplicate insert")
		}
	})

	// Test FindByID
	t.Run("FindByID", func(t *testing.T) {
		player, err := table.FindByID(uint32(1))
		if err != nil {
			t.Errorf("FindByID failed: %v", err)
		}

		if player.Name != "Alice" {
			t.Errorf("Expected name 'Alice', got '%s'", player.Name)
		}

		// Test not found
		_, err = table.FindByID(uint32(999))
		if err == nil {
			t.Error("Expected error for non-existent ID")
		}

		// Test invalid key type
		_, err = table.FindByID("invalid")
		if err == nil {
			t.Error("Expected error for invalid primary key type")
		}
	})

	// Test Update
	t.Run("Update", func(t *testing.T) {
		updatedPlayer := SimplePlayer{
			ID:     1,
			Name:   "Alice Updated",
			Score:  2000,
			Active: false,
		}

		err := table.Update(uint32(1), updatedPlayer)
		if err != nil {
			t.Errorf("Update failed: %v", err)
		}

		// Verify update
		player, err := table.FindByID(uint32(1))
		if err != nil {
			t.Errorf("FindByID after update failed: %v", err)
		}

		if player.Name != "Alice Updated" {
			t.Errorf("Expected updated name 'Alice Updated', got '%s'", player.Name)
		}

		if player.Score != 2000 {
			t.Errorf("Expected updated score 2000, got %d", player.Score)
		}

		// Test update non-existent
		err = table.Update(uint32(999), updatedPlayer)
		if err == nil {
			t.Error("Expected error for updating non-existent entity")
		}
	})

	// Test Delete
	t.Run("Delete", func(t *testing.T) {
		err := table.Delete(uint32(1))
		if err != nil {
			t.Errorf("Delete failed: %v", err)
		}

		// Verify deletion
		if table.Count() != 0 {
			t.Errorf("Expected count 0 after delete, got %d", table.Count())
		}

		// Test delete non-existent
		err = table.Delete(uint32(1))
		if err == nil {
			t.Error("Expected error for deleting non-existent entity")
		}
	})
}

func TestSimpleTableBatchOperations(t *testing.T) {
	table, err := NewSimpleTable[SimplePlayer]("batch_players")
	if err != nil {
		t.Fatalf("Failed to create table: %v", err)
	}

	t.Run("InsertBatch", func(t *testing.T) {
		players := []SimplePlayer{
			{ID: 1, Name: "Alice", Score: 1500, Active: true},
			{ID: 2, Name: "Bob", Score: 1200, Active: true},
			{ID: 3, Name: "Charlie", Score: 1800, Active: false},
		}

		err := table.InsertBatch(players)
		if err != nil {
			t.Errorf("InsertBatch failed: %v", err)
		}

		if table.Count() != 3 {
			t.Errorf("Expected count 3, got %d", table.Count())
		}

		// Test batch with duplicates
		duplicatePlayers := []SimplePlayer{
			{ID: 4, Name: "David", Score: 1400, Active: true},
			{ID: 1, Name: "Duplicate Alice", Score: 1600, Active: true}, // Duplicate
		}

		err = table.InsertBatch(duplicatePlayers)
		if err == nil {
			t.Error("Expected error for batch with existing entity")
		}

		// Test batch with internal duplicates
		internalDuplicates := []SimplePlayer{
			{ID: 5, Name: "Eve", Score: 1300, Active: true},
			{ID: 5, Name: "Duplicate Eve", Score: 1700, Active: true}, // Internal duplicate
		}

		err = table.InsertBatch(internalDuplicates)
		if err == nil {
			t.Error("Expected error for batch with internal duplicates")
		}
	})

	t.Run("FindAll", func(t *testing.T) {
		players, err := table.FindAll()
		if err != nil {
			t.Errorf("FindAll failed: %v", err)
		}

		if len(players) != 3 {
			t.Errorf("Expected 3 players, got %d", len(players))
		}

		// Verify we have the right players
		playerNames := make(map[string]bool)
		for _, player := range players {
			playerNames[player.Name] = true
		}

		expectedNames := []string{"Alice", "Bob", "Charlie"}
		for _, name := range expectedNames {
			if !playerNames[name] {
				t.Errorf("Expected player '%s' not found", name)
			}
		}
	})

	t.Run("FindWhere", func(t *testing.T) {
		// Find active players
		activePlayers, err := table.FindWhere(func(p SimplePlayer) bool {
			return p.Active
		})
		if err != nil {
			t.Errorf("FindWhere failed: %v", err)
		}

		if len(activePlayers) != 2 {
			t.Errorf("Expected 2 active players, got %d", len(activePlayers))
		}

		// Find players with score > 1400
		highScorePlayers, err := table.FindWhere(func(p SimplePlayer) bool {
			return p.Score > 1400
		})
		if err != nil {
			t.Errorf("FindWhere failed: %v", err)
		}

		if len(highScorePlayers) != 2 {
			t.Errorf("Expected 2 high-score players, got %d", len(highScorePlayers))
		}
	})
}

func TestSimpleTableUtilities(t *testing.T) {
	table, err := NewSimpleTable[SimplePlayer]("utility_test")
	if err != nil {
		t.Fatalf("Failed to create table: %v", err)
	}

	// Test empty table
	t.Run("EmptyTable", func(t *testing.T) {
		if !table.IsEmpty() {
			t.Error("Expected table to be empty")
		}

		if table.Count() != 0 {
			t.Errorf("Expected count 0, got %d", table.Count())
		}
	})

	// Add some data
	players := []SimplePlayer{
		{ID: 1, Name: "Alice", Score: 1500, Active: true},
		{ID: 2, Name: "Bob", Score: 1200, Active: true},
	}
	table.InsertBatch(players)

	t.Run("NonEmptyTable", func(t *testing.T) {
		if table.IsEmpty() {
			t.Error("Expected table to not be empty")
		}

		if table.Count() != 2 {
			t.Errorf("Expected count 2, got %d", table.Count())
		}
	})

	t.Run("Clear", func(t *testing.T) {
		table.Clear()

		if !table.IsEmpty() {
			t.Error("Expected table to be empty after clear")
		}

		if table.Count() != 0 {
			t.Errorf("Expected count 0 after clear, got %d", table.Count())
		}
	})

	t.Run("Stats", func(t *testing.T) {
		// Add data back
		table.Insert(players[0])

		stats := table.Stats()
		if stats.TableName != "utility_test" {
			t.Errorf("Expected table name 'utility_test', got '%s'", stats.TableName)
		}

		if stats.EntityType != "SimplePlayer" {
			t.Errorf("Expected entity type 'SimplePlayer', got '%s'", stats.EntityType)
		}

		if !stats.HasPrimaryKey {
			t.Error("Expected table to have primary key")
		}

		if stats.EntityCount != 1 {
			t.Errorf("Expected entity count 1, got %d", stats.EntityCount)
		}

		if stats.PrimaryKeyType != "uint32" {
			t.Errorf("Expected primary key type 'uint32', got '%s'", stats.PrimaryKeyType)
		}
	})
}

func TestSimpleTableGlobalRegistry(t *testing.T) {
	// Test GetSimpleTable factory function
	t.Run("GetSimpleTableFactory", func(t *testing.T) {
		table1, err := GetSimpleTable[SimpleGame]("factory_test")
		if err != nil {
			t.Fatalf("Failed to get table: %v", err)
		}

		// Second call should return the same instance
		table2, err := GetSimpleTable[SimpleGame]("factory_test")
		if err != nil {
			t.Fatalf("Failed to get table second time: %v", err)
		}

		// Should be the same instance (registered globally)
		if table1 != table2 {
			t.Error("Expected same table instance from global registry")
		}
	})

	// Test MustGetSimpleTable
	t.Run("MustGetSimpleTable", func(t *testing.T) {
		// Should not panic for valid entity
		table := MustGetSimpleTable[SimpleGame]("must_test")
		if table.Name() != "must_test" {
			t.Errorf("Expected table name 'must_test', got '%s'", table.Name())
		}
	})

	// Test direct registration
	t.Run("DirectRegistration", func(t *testing.T) {
		table, err := NewSimpleTable[SimplePlayer]("direct_test")
		if err != nil {
			t.Fatalf("Failed to create table: %v", err)
		}

		RegisterGlobalSimpleTable("direct_test", table)

		retrieved, exists := GetGlobalSimpleTable[SimplePlayer]("direct_test")
		if !exists {
			t.Error("Table not found in global registry")
		}

		if retrieved.Name() != "direct_test" {
			t.Errorf("Expected table name 'direct_test', got '%s'", retrieved.Name())
		}

		// Should be the same instance
		if retrieved != table {
			t.Error("Expected same table instance from global registry")
		}
	})
}

// Benchmark tests
func BenchmarkSimpleTableCreation(b *testing.B) {
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := NewSimpleTable[SimplePlayer]("bench_table")
		if err != nil {
			b.Fatalf("Table creation failed: %v", err)
		}
	}
}

func BenchmarkSimpleTableInsert(b *testing.B) {
	table, err := NewSimpleTable[SimplePlayer]("bench_insert")
	if err != nil {
		b.Fatalf("Failed to create table: %v", err)
	}

	// Clear table for consistent benchmarking
	table.Clear()

	player := SimplePlayer{
		ID:     1,
		Name:   "BenchPlayer",
		Score:  1000,
		Active: true,
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		// Use different IDs to avoid duplicate errors
		player.ID = uint32(i)
		table.Insert(player)
	}
}

func BenchmarkSimpleTableFindByID(b *testing.B) {
	table, err := NewSimpleTable[SimplePlayer]("bench_find")
	if err != nil {
		b.Fatalf("Failed to create table: %v", err)
	}

	// Populate table with test data
	for i := 0; i < 1000; i++ {
		player := SimplePlayer{
			ID:     uint32(i),
			Name:   "Player",
			Score:  1000,
			Active: true,
		}
		table.Insert(player)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		// Find random player
		id := uint32(i % 1000)
		_, err := table.FindByID(id)
		if err != nil {
			b.Fatalf("FindByID failed: %v", err)
		}
	}
}

func BenchmarkSimpleTableInsertBatch(b *testing.B) {
	table, err := NewSimpleTable[SimplePlayer]("bench_batch")
	if err != nil {
		b.Fatalf("Failed to create table: %v", err)
	}

	// Create batch of 100 players
	players := make([]SimplePlayer, 100)
	for i := range players {
		players[i] = SimplePlayer{
			ID:     uint32(i),
			Name:   "Player",
			Score:  1000,
			Active: true,
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		// Clear and repopulate for each iteration
		table.Clear()
		table.InsertBatch(players)
	}
}

func BenchmarkSimpleTableFindWhere(b *testing.B) {
	table, err := NewSimpleTable[SimplePlayer]("bench_where")
	if err != nil {
		b.Fatalf("Failed to create table: %v", err)
	}

	// Populate table with test data
	for i := 0; i < 1000; i++ {
		player := SimplePlayer{
			ID:     uint32(i),
			Name:   "Player",
			Score:  uint32(1000 + i%500), // Scores from 1000-1499
			Active: i%2 == 0,
		}
		table.Insert(player)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		// Find players with high scores
		_, err := table.FindWhere(func(p SimplePlayer) bool {
			return p.Score > 1300
		})
		if err != nil {
			b.Fatalf("FindWhere failed: %v", err)
		}
	}
}
