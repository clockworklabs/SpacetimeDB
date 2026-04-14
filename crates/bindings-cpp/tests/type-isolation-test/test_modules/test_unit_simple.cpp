// Simplified test module for unit struct types
#include "spacetimedb.h"
#include <cstdint>

using namespace SpacetimeDB;

// Define a single unit struct
SPACETIMEDB_UNIT_STRUCT(BasicUnit)

// Simple table with unit field
struct SimpleTable {
    uint32_t id;
    BasicUnit unit;
    int32_t value;
};
SPACETIMEDB_STRUCT(SimpleTable, id, unit, value)
SPACETIMEDB_TABLE(SimpleTable, simple_table, Public)

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    BasicUnit unit{};
    ctx.db[simple_table].insert({1, unit, 100});
    return Ok();
}