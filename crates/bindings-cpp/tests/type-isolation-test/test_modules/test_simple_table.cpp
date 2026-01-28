#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    LOG_INFO("Test simple table initialized");
    return Ok();
}

// Simplest possible table - no optional fields
struct SimpleTable {
    uint32_t id;
    int32_t value;
};
SPACETIMEDB_STRUCT(SimpleTable, id, value)
SPACETIMEDB_TABLE(SimpleTable, simple_table, Public)