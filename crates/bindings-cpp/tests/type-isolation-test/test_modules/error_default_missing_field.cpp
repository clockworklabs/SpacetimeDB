#include <spacetimedb.h>

using namespace SpacetimeDB;

struct BadDefaultRow {
    uint32_t id;
    uint32_t player;
};
SPACETIMEDB_STRUCT(BadDefaultRow, id, player)
SPACETIMEDB_TABLE(BadDefaultRow, bad_default_row, Public)
FIELD_PrimaryKey(bad_default_row, id)

// Negative test: "missing_col" does not exist in BadDefaultRow.
FIELD_Default(bad_default_row, missing_col, uint32_t(7))

SPACETIMEDB_REDUCER(insert_bad_default_row, ReducerContext ctx, uint32_t id, uint32_t player)
{
    ctx.db[bad_default_row].insert(BadDefaultRow{id, player});
    return Ok();
}

