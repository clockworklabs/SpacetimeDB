#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDb;

SPACETIMEDB_INIT(init) {
    LOG_INFO("Test simple table initialized");
}

// Simplest possible table - no optional fields
struct SimpleTable {
    uint32_t id;
    int32_t value;
};
SPACETIMEDB_STRUCT(SimpleTable, id, value)
SPACETIMEDB_TABLE(SimpleTable, simple_table, Public)