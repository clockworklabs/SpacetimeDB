#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDB;

// Test: Optional in reducer only (no table fields)


// Struct with optional field (used as reducer parameter)
struct OptionalParam {
    int32_t id;
    std::optional<int32_t> opt_value;
};
SPACETIMEDB_STRUCT(OptionalParam, id, opt_value)

// Simple non-optional table
struct SimpleTable {
    int32_t id;
    int32_t value;
};
SPACETIMEDB_STRUCT(SimpleTable, id, value)
SPACETIMEDB_TABLE(SimpleTable, simple_table, Public)

// Reducer that uses struct with optional as parameter
SPACETIMEDB_REDUCER(insert_with_optional, ReducerContext ctx, OptionalParam param)
{
    SimpleTable row;
    row.id = param.id;
    // Use the optional value or default to 0
    row.value = param.opt_value.value_or(0);
    ctx.db.table<SimpleTable>("simple_table").insert(row);
}

// Another reducer without optional
SPACETIMEDB_REDUCER(insert_direct, ReducerContext ctx, int32_t id, int32_t value)
{
    SimpleTable row;
    row.id = id;
    row.value = value;
    ctx.db.table<SimpleTable>("simple_table").insert(row);
}