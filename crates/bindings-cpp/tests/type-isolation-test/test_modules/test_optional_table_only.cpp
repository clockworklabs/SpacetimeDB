#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDB;

// Test: Optional in table only (no reducer parameters)


// Simple optional table
struct OptionalI32Table {
    std::optional<int32_t> value;
};
SPACETIMEDB_STRUCT(OptionalI32Table, value)
SPACETIMEDB_TABLE(OptionalI32Table, optional_i32_table, Public)

// Reducer that doesn't use optional as parameter
SPACETIMEDB_REDUCER(insert_value, ReducerContext ctx, int32_t v)
{
    OptionalI32Table row;
    row.value = v;  // Convert to optional
    ctx.db.table<OptionalI32Table>("optional_i32_table").insert(row);
}

SPACETIMEDB_REDUCER(insert_none, ReducerContext ctx)
{
    OptionalI32Table row;
    row.value = std::nullopt;
    ctx.db.table<OptionalI32Table>("optional_i32_table").insert(row);
}