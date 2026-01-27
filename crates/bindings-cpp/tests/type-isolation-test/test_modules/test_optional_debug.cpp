#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDB;

// MINIMAL TEST: Just optional in table field, NO reducer parameters
// This isolates whether issue is in table registration vs reducer parameter processing

struct OptionalTable {
    std::optional<int32_t> maybe_value;
};
SPACETIMEDB_STRUCT(OptionalTable, maybe_value)
SPACETIMEDB_TABLE(OptionalTable, optional_table, Public)

// Simple reducer that doesn't use optional as parameter
SPACETIMEDB_REDUCER(test_basic, ReducerContext ctx)
{
    LOG_INFO("Basic reducer called");
}