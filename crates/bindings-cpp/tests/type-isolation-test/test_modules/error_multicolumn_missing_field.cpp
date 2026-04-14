#include <spacetimedb.h>

using namespace SpacetimeDB;

struct BadIndexRow {
    uint32_t id;
    uint32_t player;
};
SPACETIMEDB_STRUCT(BadIndexRow, id, player)
SPACETIMEDB_TABLE(BadIndexRow, bad_index_row, Public)
FIELD_PrimaryKey(bad_index_row, id)

// Negative test: "round" does not exist in BadIndexRow.
FIELD_MultiColumnIndex(bad_index_row, by_player_round, player, round)

SPACETIMEDB_REDUCER(insert_bad_index_row, ReducerContext ctx, uint32_t id, uint32_t player)
{
    ctx.db[bad_index_row].insert(BadIndexRow{id, player});
    return Ok();
}

