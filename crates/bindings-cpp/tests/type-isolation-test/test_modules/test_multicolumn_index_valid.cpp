#include <spacetimedb.h>

using namespace SpacetimeDB;

struct ScoreRow {
    uint32_t id;
    uint32_t player;
    uint32_t round;
    int32_t score;
};
SPACETIMEDB_STRUCT(ScoreRow, id, player, round, score)
SPACETIMEDB_TABLE(ScoreRow, score_row, Public)
FIELD_PrimaryKey(score_row, id)
FIELD_MultiColumnIndex(score_row, by_player_round, player, round)

SPACETIMEDB_REDUCER(insert_score, ReducerContext ctx, uint32_t id, uint32_t player, uint32_t round, int32_t score)
{
    ctx.db[score_row].insert(ScoreRow{id, player, round, score});
    return Ok();
}

