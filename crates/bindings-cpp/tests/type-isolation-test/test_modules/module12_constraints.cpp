#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 12: Constraint tables (Indexes, etc.)
// Testing if constraint-based tables cause WASM issues


// IndexedTable
struct IndexedTable {
    uint32_t player_id;
};
SPACETIMEDB_STRUCT(IndexedTable, player_id)
SPACETIMEDB_TABLE(IndexedTable, indexed_table, Private)
FIELD_Index(indexed_table, player_id)

// IndexedTable2 with multiple indexes
struct IndexedTable2 {
    uint32_t player_id;
    float player_snazz;
};
SPACETIMEDB_STRUCT(IndexedTable2, player_id, player_snazz)
SPACETIMEDB_TABLE(IndexedTable2, indexed_table_2, Private)
FIELD_Index(indexed_table_2, player_id)
// Note: float fields cannot be indexed - removing this constraint
// FIELD_Index(indexed_table_2, player_snazz)  // Would fail compile-time validation

// BTreeU32 table with index
struct BTreeU32 {
    uint32_t n;
    int32_t data;
};
SPACETIMEDB_STRUCT(BTreeU32, n, data)
SPACETIMEDB_TABLE(BTreeU32, btree_u32, Public)
FIELD_Index(btree_u32, n)

// PkU32Two - additional primary key table
struct PkU32Two { uint32_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU32Two, n, data)
SPACETIMEDB_TABLE(PkU32Two, pk_u32_two, Public)
FIELD_PrimaryKey(pk_u32_two, n)

// Reducers for constraint tables
SPACETIMEDB_REDUCER(insert_indexed_table, ReducerContext ctx, uint32_t player_id)
{
    ctx.db.table<IndexedTable>("indexed_table").insert(IndexedTable{player_id});
}

SPACETIMEDB_REDUCER(insert_indexed_table_2, ReducerContext ctx, uint32_t player_id, float player_snazz)
{
    ctx.db.table<IndexedTable2>("indexed_table_2").insert(IndexedTable2{player_id, player_snazz});
}

SPACETIMEDB_REDUCER(insert_btree_u32, ReducerContext ctx, uint32_t n, int32_t data)
{
    ctx.db.table<BTreeU32>("btree_u32").insert(BTreeU32{n, data});
}