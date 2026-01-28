#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)
#include <optional>

using namespace SpacetimeDB;


// Simple table with optional field
struct SimpleOptional {
    uint32_t id;
    std::optional<int32_t> maybe_value;
};

SPACETIMEDB_STRUCT(SimpleOptional, id, maybe_value)
SPACETIMEDB_TABLE(SimpleOptional, simple_optional, Public)

// Parameter wrapper to avoid direct optional parameter
struct OptionalParam {
    uint32_t id;
    std::optional<int32_t> value;
};
SPACETIMEDB_STRUCT(OptionalParam, id, value)

// Reducer to insert optional value (using struct wrapper instead of direct optional parameter)
SPACETIMEDB_REDUCER(insert_optional, ReducerContext ctx, OptionalParam param)
{
    ctx.db.table<SimpleOptional>("simple_optional").insert(SimpleOptional{param.id, param.value});
}