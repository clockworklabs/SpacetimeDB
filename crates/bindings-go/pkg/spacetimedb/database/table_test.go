package database

import (
	"testing"
)

// Test entity types
type Player struct {
	ID     uint32 `spacetime:"primary_key"`
	Name   string
	Score  uint32
	Active bool
}

type Game struct {
	ID    uint32 `spacetime:"primary_key"`
	Name  string
	Round uint32
}

func TestTableAccessorCreation(t *testing.T) {
	// Test successful table creation
	t.Run("ValidEntity", func(t *testing.T) {
		table, err := NewTableAccessor[Player]("players", nil)
		if err != nil {
			t.Fatalf("Failed to create table: %v", err)
		}

		if table.Name() != "players" {
			t.Errorf("Expected table name 'players', got '%s'", table.Name())
		}

		stats := table.Stats()
		if stats.EntityType != "Player" {
			t.Errorf("Expected entity type 'Player', got '%s'", stats.EntityType)
		}

		if !stats.HasPrimaryKey {
			t.Error("Expected table to have primary key")
		}
	})

	// Test entity without struct
	t.Run("InvalidEntityType", func(t *testing.T) {
		_, err := NewTableAccessor[string]("invalid", nil)
		if err == nil {
			t.Error("Expected error for non-struct entity type")
		}

		if tableErr, ok := err.(*TableError); ok {
			if tableErr.Operation != "create" {
				t.Errorf("Expected operation 'create', got '%s'", tableErr.Operation)
			}
		} else {
			t.Errorf("Expected TableError, got %T", err)
		}
	})
}

func TestTableOperations(t *testing.T) {
	table, err := NewTableAccessor[Player]("test_players", nil)
	if err != nil {
		t.Fatalf("Failed to create table: %v", err)
	}

	// Test Insert
	t.Run("Insert", func(t *testing.T) {
		player := Player{
			ID:     1,
			Name:   "Alice",
			Score:  1500,
			Active: true,
		}

		err := table.Insert(player)
		if err != nil {
			t.Errorf("Insert failed: %v", err)
		}
	})

	// Test InsertBatch
	t.Run("InsertBatch", func(t *testing.T) {
		players := []Player{
			{ID: 2, Name: "Bob", Score: 1200, Active: true},
			{ID: 3, Name: "Charlie", Score: 1800, Active: false},
		}

		err := table.InsertBatch(players)
		if err != nil {
			t.Errorf("InsertBatch failed: %v", err)
		}
	})

	// Test empty batch
	t.Run("InsertEmptyBatch", func(t *testing.T) {
		var players []Player
		err := table.InsertBatch(players)
		if err != nil {
			t.Errorf("InsertBatch with empty slice should not fail: %v", err)
		}
	})

	// Test primary key validation
	t.Run("ValidatePrimaryKey", func(t *testing.T) {
		// Valid key
		_, err := table.FindByID(uint32(42))
		// Should fail with "not implemented yet" error, not validation error
		if err == nil {
			t.Error("Expected error since FindByID is not implemented")
		}

		// Invalid key type
		_, err = table.FindByID("invalid")
		if err == nil {
			t.Error("Expected error for invalid primary key type")
		}

		if tableErr, ok := err.(*TableError); ok {
			if tableErr.Reason != "invalid primary key" {
				t.Errorf("Expected 'invalid primary key' reason, got '%s'", tableErr.Reason)
			}
		}
	})
}

func TestGlobalRegistry(t *testing.T) {
	// Test registration and retrieval
	t.Run("RegisterAndRetrieve", func(t *testing.T) {
		table, err := NewTableAccessor[Game]("games", nil)
		if err != nil {
			t.Fatalf("Failed to create table: %v", err)
		}

		RegisterGlobalTable("games", table)

		retrieved, exists := GetGlobalTable[Game]("games")
		if !exists {
			t.Error("Table not found in global registry")
		}

		if retrieved.Name() != "games" {
			t.Errorf("Expected table name 'games', got '%s'", retrieved.Name())
		}
	})

	// Test GetTable factory function
	t.Run("GetTableFactory", func(t *testing.T) {
		table1, err := GetTable[Player]("factory_test")
		if err != nil {
			t.Fatalf("Failed to get table: %v", err)
		}

		// Second call should return the same instance
		table2, err := GetTable[Player]("factory_test")
		if err != nil {
			t.Fatalf("Failed to get table second time: %v", err)
		}

		// Should be the same instance (registered globally)
		if table1 != table2 {
			t.Error("Expected same table instance from global registry")
		}
	})

	// Test MustGetTable
	t.Run("MustGetTable", func(t *testing.T) {
		// Should not panic for valid entity
		table := MustGetTable[Game]("must_test")
		if table.Name() != "must_test" {
			t.Errorf("Expected table name 'must_test', got '%s'", table.Name())
		}
	})

	// Test ListGlobalTables
	t.Run("ListTables", func(t *testing.T) {
		// Create a few tables
		GetTable[Player]("list_test_1")
		GetTable[Game]("list_test_2")

		tables := ListGlobalTables()
		if len(tables) < 2 {
			t.Errorf("Expected at least 2 tables, got %d", len(tables))
		}

		// Check that our test tables are in the list
		found1, found2 := false, false
		for _, name := range tables {
			if name == "list_test_1" {
				found1 = true
			}
			if name == "list_test_2" {
				found2 = true
			}
		}

		if !found1 || !found2 {
			t.Errorf("Test tables not found in global list: %v", tables)
		}
	})
}

func TestTableStats(t *testing.T) {
	table, err := NewTableAccessor[Player]("stats_test", nil)
	if err != nil {
		t.Fatalf("Failed to create table: %v", err)
	}

	stats := table.Stats()

	// Validate stats
	if stats.TableName != "stats_test" {
		t.Errorf("Expected table name 'stats_test', got '%s'", stats.TableName)
	}

	if stats.EntityType != "Player" {
		t.Errorf("Expected entity type 'Player', got '%s'", stats.EntityType)
	}

	if !stats.HasPrimaryKey {
		t.Error("Expected table to have primary key")
	}

	// Player should have 4 columns: ID, Name, Score, Active
	if stats.ColumnCount != 4 {
		t.Errorf("Expected 4 columns, got %d", stats.ColumnCount)
	}
}

func TestSchemaGeneration(t *testing.T) {
	table, err := NewTableAccessor[Player]("schema_test", nil)
	if err != nil {
		t.Fatalf("Failed to create table: %v", err)
	}

	schema := table.Schema()
	if schema == nil {
		t.Fatal("Schema should not be nil")
	}

	if schema.Name != "schema_test" {
		t.Errorf("Expected schema name 'schema_test', got '%s'", schema.Name)
	}

	// Check that primary key column exists
	foundPK := false
	for _, col := range schema.Columns {
		if col.Name == "ID" && col.PrimaryKey {
			foundPK = true
			break
		}
	}

	if !foundPK {
		t.Error("Primary key column 'ID' not found in schema")
	}
}

// Benchmark tests
func BenchmarkTableCreation(b *testing.B) {
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := NewTableAccessor[Player]("bench_table", nil)
		if err != nil {
			b.Fatalf("Table creation failed: %v", err)
		}
	}
}

func BenchmarkTableInsert(b *testing.B) {
	table, err := NewTableAccessor[Player]("bench_insert", nil)
	if err != nil {
		b.Fatalf("Failed to create table: %v", err)
	}

	player := Player{
		ID:     1,
		Name:   "BenchPlayer",
		Score:  1000,
		Active: true,
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		table.Insert(player)
	}
}

func BenchmarkTableInsertBatch(b *testing.B) {
	table, err := NewTableAccessor[Player]("bench_batch", nil)
	if err != nil {
		b.Fatalf("Failed to create table: %v", err)
	}

	// Create batch of 100 players
	players := make([]Player, 100)
	for i := range players {
		players[i] = Player{
			ID:     uint32(i),
			Name:   "Player",
			Score:  1000,
			Active: true,
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		table.InsertBatch(players)
	}
}
