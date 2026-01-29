#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 10: Vector/array types (already covered in other modules, adding nested)
// Testing if nested vector types cause WASM issues


// Tables that hold other table structs  
struct OneU8 { uint8_t n; };
SPACETIMEDB_STRUCT(OneU8, n)

struct VecU8 { std::vector<uint8_t> n; };
SPACETIMEDB_STRUCT(VecU8, n)

// TableHoldsTable - nested table structures
struct TableHoldsTable {
    OneU8 a;
    VecU8 b;
};
SPACETIMEDB_STRUCT(TableHoldsTable, a, b)
SPACETIMEDB_TABLE(TableHoldsTable, table_holds_table, Public)

// Test some additional vector types not covered elsewhere
struct VecVecU8 { std::vector<std::vector<uint8_t>> n; };
SPACETIMEDB_STRUCT(VecVecU8, n)
SPACETIMEDB_TABLE(VecVecU8, vec_vec_u8, Public)

struct VecVecString { std::vector<std::vector<std::string>> s; };
SPACETIMEDB_STRUCT(VecVecString, s)
SPACETIMEDB_TABLE(VecVecString, vec_vec_string, Public)

// Reducers for nested vector types
SPACETIMEDB_REDUCER(insert_table_holds_table, ReducerContext ctx, OneU8 a, VecU8 b)
{
    ctx.db.table<TableHoldsTable>("table_holds_table").insert(TableHoldsTable{a, b});
}

// Direct vector parameter reducers
SPACETIMEDB_REDUCER(insert_vec_u8, ReducerContext ctx, std::vector<uint8_t> n)
{
    LOG_INFO("Received vector<uint8_t> parameter");
}

SPACETIMEDB_REDUCER(insert_vec_vec_u8, ReducerContext ctx, std::vector<std::vector<uint8_t>> n)
{
    ctx.db.table<VecVecU8>("vec_vec_u8").insert(VecVecU8{n});
}

SPACETIMEDB_REDUCER(insert_vec_vec_string, ReducerContext ctx, std::vector<std::vector<std::string>> s)
{
    ctx.db.table<VecVecString>("vec_vec_string").insert(VecVecString{s});
}