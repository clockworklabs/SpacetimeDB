package cache_test

import (
	"sync/atomic"
	"testing"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/cache"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/protocol"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// testRow is a simple row type for testing.
type testRow struct {
	ID   uint32
	Name string
}

// testTableDef implements cache.TableDef for testing.
type testTableDef struct {
	name string
}

func (d *testTableDef) TableName() string { return d.name }

func (d *testTableDef) DecodeRow(r bsatn.Reader) (any, error) {
	id, err := r.GetU32()
	if err != nil {
		return nil, err
	}
	name, err := r.GetString()
	if err != nil {
		return nil, err
	}
	return &testRow{ID: id, Name: name}, nil
}

func (d *testTableDef) EncodeRow(row any) []byte {
	tr := row.(*testRow)
	w := bsatn.NewWriter(32)
	w.PutU32(tr.ID)
	w.PutString(tr.Name)
	return w.Bytes()
}

func encodeTestRow(id uint32, name string) []byte {
	w := bsatn.NewWriter(32)
	w.PutU32(id)
	w.PutString(name)
	return w.Bytes()
}

// --- ClientCache tests ---

func TestClientCache_RegisterTable_GetTable(t *testing.T) {
	cc := cache.NewClientCache()

	def := &testTableDef{name: "users"}
	cc.RegisterTable(def)

	tc := cc.GetTable("users")
	require.NotNil(t, tc, "GetTable should return a non-nil TableCache after RegisterTable")

	assert.Equal(t, 0, tc.Count(), "new table should have 0 rows")
}

func TestClientCache_GetTable_NotFound(t *testing.T) {
	cc := cache.NewClientCache()

	tc := cc.GetTable("nonexistent")
	assert.Nil(t, tc, "GetTable should return nil for unregistered table")
}

func TestClientCache_RegisterTable_MultipleTables(t *testing.T) {
	cc := cache.NewClientCache()

	cc.RegisterTable(&testTableDef{name: "users"})
	cc.RegisterTable(&testTableDef{name: "items"})
	cc.RegisterTable(&testTableDef{name: "events"})

	assert.NotNil(t, cc.GetTable("users"))
	assert.NotNil(t, cc.GetTable("items"))
	assert.NotNil(t, cc.GetTable("events"))
	assert.Nil(t, cc.GetTable("other"))
}

// --- ApplySubscribeApplied tests ---

func TestClientCache_ApplySubscribeApplied(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	row1 := encodeTestRow(1, "alice")
	row2 := encodeTestRow(2, "bobby")
	allRows := append(row1, row2...)

	rows := &protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "users",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.RowOffsetsHint{Offsets: []uint64{0, uint64(len(row1))}},
					RowsData: allRows,
				},
			},
		},
	}

	cc.ApplySubscribeApplied(rows)

	tc := cc.GetTable("users")
	require.NotNil(t, tc)
	assert.Equal(t, 2, tc.Count())
}

func TestClientCache_ApplySubscribeApplied_NilRows(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	// Should not panic
	cc.ApplySubscribeApplied(nil)

	tc := cc.GetTable("users")
	require.NotNil(t, tc)
	assert.Equal(t, 0, tc.Count())
}

func TestClientCache_ApplySubscribeApplied_UnknownTable(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	rows := &protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "unknown_table",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: 4},
					RowsData: []byte{0x01, 0x02, 0x03, 0x04},
				},
			},
		},
	}

	// Should not panic on unknown table
	cc.ApplySubscribeApplied(rows)

	tc := cc.GetTable("users")
	require.NotNil(t, tc)
	assert.Equal(t, 0, tc.Count(), "users table should still be empty")
}

// --- ApplyTransactionUpdate tests ---

func TestClientCache_ApplyTransactionUpdate_Insert(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "players"})

	row1 := encodeTestRow(10, "charlie")

	update := &protocol.TransactionUpdate{
		QuerySets: []protocol.QuerySetUpdate{
			{
				QuerySetID: 1,
				Tables: []protocol.TableUpdate{
					{
						TableName: "players",
						Rows: []protocol.TableUpdateRows{
							&protocol.PersistentTableRows{
								Inserts: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row1))},
									RowsData: row1,
								},
								Deletes: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row1))},
									RowsData: nil,
								},
							},
						},
					},
				},
			},
		},
	}

	cc.ApplyTransactionUpdate(update)

	tc := cc.GetTable("players")
	require.NotNil(t, tc)
	assert.Equal(t, 1, tc.Count())
}

func TestClientCache_ApplyTransactionUpdate_Delete(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "players"})

	row1 := encodeTestRow(10, "charlie")

	// First insert the row via subscribe
	subscribeRows := &protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "players",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row1))},
					RowsData: row1,
				},
			},
		},
	}
	cc.ApplySubscribeApplied(subscribeRows)

	tc := cc.GetTable("players")
	require.NotNil(t, tc)
	require.Equal(t, 1, tc.Count())

	// Now delete it via transaction update
	update := &protocol.TransactionUpdate{
		QuerySets: []protocol.QuerySetUpdate{
			{
				QuerySetID: 1,
				Tables: []protocol.TableUpdate{
					{
						TableName: "players",
						Rows: []protocol.TableUpdateRows{
							&protocol.PersistentTableRows{
								Inserts: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row1))},
									RowsData: nil,
								},
								Deletes: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row1))},
									RowsData: row1,
								},
							},
						},
					},
				},
			},
		},
	}

	cc.ApplyTransactionUpdate(update)
	assert.Equal(t, 0, tc.Count())
}

func TestClientCache_ApplyTransactionUpdate_Nil(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "players"})

	// Should not panic
	cc.ApplyTransactionUpdate(nil)
}

// --- TableCache tests ---

func TestTableCache_Count(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "items"})

	row1 := encodeTestRow(1, "sword")
	row2 := encodeTestRow(2, "shield")
	allRows := append(row1, row2...)

	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "items",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.RowOffsetsHint{Offsets: []uint64{0, uint64(len(row1))}},
					RowsData: allRows,
				},
			},
		},
	})

	tc := cc.GetTable("items")
	require.NotNil(t, tc)
	assert.Equal(t, 2, tc.Count())
}

func TestTableCache_Iter(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "items"})

	row1 := encodeTestRow(1, "sword")
	row2 := encodeTestRow(2, "shield")
	allRows := append(row1, row2...)

	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "items",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.RowOffsetsHint{Offsets: []uint64{0, uint64(len(row1))}},
					RowsData: allRows,
				},
			},
		},
	})

	tc := cc.GetTable("items")
	require.NotNil(t, tc)

	var collected []*testRow
	tc.Iter(func(row any) bool {
		collected = append(collected, row.(*testRow))
		return true
	})

	assert.Len(t, collected, 2)

	// Verify we got both rows (order is not guaranteed)
	ids := map[uint32]bool{}
	for _, r := range collected {
		ids[r.ID] = true
	}
	assert.True(t, ids[1], "should contain row with ID 1")
	assert.True(t, ids[2], "should contain row with ID 2")
}

func TestTableCache_Iter_EarlyStop(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "items"})

	row1 := encodeTestRow(1, "sword")
	row2 := encodeTestRow(2, "shield")
	row3 := encodeTestRow(3, "potion")
	allRows := append(append(row1, row2...), row3...)

	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "items",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.RowOffsetsHint{Offsets: []uint64{
						0,
						uint64(len(row1)),
						uint64(len(row1) + len(row2)),
					}},
					RowsData: allRows,
				},
			},
		},
	})

	tc := cc.GetTable("items")
	require.NotNil(t, tc)

	count := 0
	tc.Iter(func(row any) bool {
		count++
		return false // stop after first
	})

	assert.Equal(t, 1, count, "iteration should stop after returning false")
}

// --- Callback tests ---

func TestTableCache_OnInsert_Callback(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	tc := cc.GetTable("users")
	require.NotNil(t, tc)

	var insertedRow atomic.Value
	tc.OnInsert(func(row any) {
		insertedRow.Store(row)
	})

	row := encodeTestRow(1, "alice")
	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "users",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
					RowsData: row,
				},
			},
		},
	})

	stored := insertedRow.Load()
	require.NotNil(t, stored, "insert callback should have fired")
	tr := stored.(*testRow)
	assert.Equal(t, uint32(1), tr.ID)
	assert.Equal(t, "alice", tr.Name)
}

func TestTableCache_OnDelete_Callback(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	tc := cc.GetTable("users")
	require.NotNil(t, tc)

	var deletedRow atomic.Value
	tc.OnDelete(func(row any) {
		deletedRow.Store(row)
	})

	row := encodeTestRow(1, "alice")

	// Insert via subscribe
	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "users",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
					RowsData: row,
				},
			},
		},
	})
	require.Equal(t, 1, tc.Count())

	// Delete via transaction update
	cc.ApplyTransactionUpdate(&protocol.TransactionUpdate{
		QuerySets: []protocol.QuerySetUpdate{
			{
				QuerySetID: 1,
				Tables: []protocol.TableUpdate{
					{
						TableName: "users",
						Rows: []protocol.TableUpdateRows{
							&protocol.PersistentTableRows{
								Inserts: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
									RowsData: nil,
								},
								Deletes: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
									RowsData: row,
								},
							},
						},
					},
				},
			},
		},
	})

	stored := deletedRow.Load()
	require.NotNil(t, stored, "delete callback should have fired")
	tr := stored.(*testRow)
	assert.Equal(t, uint32(1), tr.ID)
	assert.Equal(t, "alice", tr.Name)
	assert.Equal(t, 0, tc.Count())
}

func TestTableCache_RemoveCallback(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	tc := cc.GetTable("users")
	require.NotNil(t, tc)

	var callCount atomic.Int32
	cbID := tc.OnInsert(func(row any) {
		callCount.Add(1)
	})

	row := encodeTestRow(1, "alice")

	// First insert should fire callback
	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "users",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
					RowsData: row,
				},
			},
		},
	})
	assert.Equal(t, int32(1), callCount.Load())

	// Remove callback
	tc.RemoveCallback(cbID)

	// Second insert should NOT fire callback
	row2 := encodeTestRow(2, "bob")
	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "users",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row2))},
					RowsData: row2,
				},
			},
		},
	})
	assert.Equal(t, int32(1), callCount.Load(), "callback should not fire after removal")
}

func TestTableCache_MultipleInsertCallbacks(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	tc := cc.GetTable("users")
	require.NotNil(t, tc)

	var count1, count2 atomic.Int32
	tc.OnInsert(func(row any) {
		count1.Add(1)
	})
	tc.OnInsert(func(row any) {
		count2.Add(1)
	})

	row := encodeTestRow(1, "alice")
	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "users",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
					RowsData: row,
				},
			},
		},
	})

	assert.Equal(t, int32(1), count1.Load(), "first callback should fire")
	assert.Equal(t, int32(1), count2.Load(), "second callback should fire")
}

// --- Concurrent access tests ---

func TestClientCache_ConcurrentInserts(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "users"})

	tc := cc.GetTable("users")
	require.NotNil(t, tc)

	var insertCount atomic.Int32
	tc.OnInsert(func(row any) {
		insertCount.Add(1)
	})

	// Insert rows concurrently from multiple goroutines
	const numGoroutines = 10
	const rowsPerGoroutine = 50
	done := make(chan struct{}, numGoroutines)

	for g := 0; g < numGoroutines; g++ {
		go func(goroutineID int) {
			defer func() { done <- struct{}{} }()
			for i := 0; i < rowsPerGoroutine; i++ {
				id := uint32(goroutineID*rowsPerGoroutine + i)
				row := encodeTestRow(id, "user")
				cc.ApplySubscribeApplied(&protocol.QueryRows{
					Tables: []protocol.SingleTableRows{
						{
							TableName: "users",
							Rows: &protocol.BsatnRowList{
								SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
								RowsData: row,
							},
						},
					},
				})
			}
		}(g)
	}

	// Wait for all goroutines
	for i := 0; i < numGoroutines; i++ {
		<-done
	}

	// All rows with name "user" have the same length, but different IDs,
	// so each produces unique bytes and should be stored separately.
	assert.Equal(t, numGoroutines*rowsPerGoroutine, tc.Count())
	assert.Equal(t, int32(numGoroutines*rowsPerGoroutine), insertCount.Load())
}

func TestClientCache_ConcurrentRegisterAndGet(t *testing.T) {
	cc := cache.NewClientCache()

	const numTables = 50
	done := make(chan struct{}, numTables*2)

	// Register tables concurrently
	for i := 0; i < numTables; i++ {
		go func(idx int) {
			defer func() { done <- struct{}{} }()
			name := "table_" + string(rune('A'+idx%26)) + string(rune('0'+idx/26))
			cc.RegisterTable(&testTableDef{name: name})
		}(i)
	}

	// Simultaneously try to get tables
	for i := 0; i < numTables; i++ {
		go func(idx int) {
			defer func() { done <- struct{}{} }()
			name := "table_" + string(rune('A'+idx%26)) + string(rune('0'+idx/26))
			// May or may not find it depending on goroutine scheduling
			_ = cc.GetTable(name)
		}(i)
	}

	// Wait for all
	for i := 0; i < numTables*2; i++ {
		<-done
	}

	// After all goroutines finish, all tables should be registered
	for i := 0; i < numTables; i++ {
		name := "table_" + string(rune('A'+i%26)) + string(rune('0'+i/26))
		assert.NotNil(t, cc.GetTable(name), "table %s should be registered", name)
	}
}

func TestTableCache_ConcurrentCallbackRegistration(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "items"})

	tc := cc.GetTable("items")
	require.NotNil(t, tc)

	const numCallbacks = 100
	var totalInserts atomic.Int32
	done := make(chan struct{}, numCallbacks)

	// Register callbacks concurrently
	for i := 0; i < numCallbacks; i++ {
		go func() {
			defer func() { done <- struct{}{} }()
			tc.OnInsert(func(row any) {
				totalInserts.Add(1)
			})
		}()
	}

	for i := 0; i < numCallbacks; i++ {
		<-done
	}

	// Insert one row -- all callbacks should fire
	row := encodeTestRow(1, "sword")
	cc.ApplySubscribeApplied(&protocol.QueryRows{
		Tables: []protocol.SingleTableRows{
			{
				TableName: "items",
				Rows: &protocol.BsatnRowList{
					SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row))},
					RowsData: row,
				},
			},
		},
	})

	assert.Equal(t, int32(numCallbacks), totalInserts.Load(),
		"all %d callbacks should have fired", numCallbacks)
}

// --- EventTableRows via TransactionUpdate ---

func TestClientCache_ApplyTransactionUpdate_EventTableRows(t *testing.T) {
	cc := cache.NewClientCache()
	cc.RegisterTable(&testTableDef{name: "events"})

	row1 := encodeTestRow(100, "event_a")

	update := &protocol.TransactionUpdate{
		QuerySets: []protocol.QuerySetUpdate{
			{
				QuerySetID: 1,
				Tables: []protocol.TableUpdate{
					{
						TableName: "events",
						Rows: []protocol.TableUpdateRows{
							&protocol.EventTableRows{
								Events: &protocol.BsatnRowList{
									SizeHint: protocol.FixedSizeHint{RowSize: uint16(len(row1))},
									RowsData: row1,
								},
							},
						},
					},
				},
			},
		},
	}

	cc.ApplyTransactionUpdate(update)

	tc := cc.GetTable("events")
	require.NotNil(t, tc)
	assert.Equal(t, 1, tc.Count())
}
