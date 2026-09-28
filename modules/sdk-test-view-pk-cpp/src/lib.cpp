#include <spacetimedb.h>

using namespace SpacetimeDB;

struct ViewPkPlayer {
    uint64_t id;
    std::string name;
};
SPACETIMEDB_STRUCT(ViewPkPlayer, id, name)
SPACETIMEDB_TABLE(ViewPkPlayer, view_pk_player, Public)
FIELD_PrimaryKey(view_pk_player, id)

struct ViewPkMembership {
    uint64_t id;
    uint64_t player_id;
};
SPACETIMEDB_STRUCT(ViewPkMembership, id, player_id)
SPACETIMEDB_TABLE(ViewPkMembership, view_pk_membership, Public)
FIELD_PrimaryKey(view_pk_membership, id)
FIELD_Index(view_pk_membership, player_id)

struct ViewPkMembershipSecondary {
    uint64_t id;
    uint64_t player_id;
};
SPACETIMEDB_STRUCT(ViewPkMembershipSecondary, id, player_id)
SPACETIMEDB_TABLE(ViewPkMembershipSecondary, view_pk_membership_secondary, Public)
FIELD_PrimaryKey(view_pk_membership_secondary, id)
FIELD_Index(view_pk_membership_secondary, player_id)

using ViewPkPlayerQuery = Query<ViewPkPlayer>;

SPACETIMEDB_REDUCER(insert_view_pk_player, ReducerContext ctx, uint64_t id, std::string name) {
    ctx.db[view_pk_player].insert(ViewPkPlayer{id, std::move(name)});
    return Ok();
}

SPACETIMEDB_REDUCER(update_view_pk_player, ReducerContext ctx, uint64_t id, std::string name) {
    ctx.db[view_pk_player_id].update(ViewPkPlayer{id, std::move(name)});
    return Ok();
}

SPACETIMEDB_REDUCER(insert_view_pk_membership, ReducerContext ctx, uint64_t id, uint64_t player_id) {
    ctx.db[view_pk_membership].insert(ViewPkMembership{id, player_id});
    return Ok();
}

SPACETIMEDB_REDUCER(insert_view_pk_membership_secondary, ReducerContext ctx, uint64_t id, uint64_t player_id) {
    ctx.db[view_pk_membership_secondary].insert(ViewPkMembershipSecondary{id, player_id});
    return Ok();
}

SPACETIMEDB_VIEW(ViewPkPlayerQuery, all_view_pk_players, Public, ViewContext ctx) {
    return ctx.from[view_pk_player];
}

SPACETIMEDB_VIEW(ViewPkPlayerQuery, sender_view_pk_players_a, Public, ViewContext ctx) {
    return ctx.from[view_pk_membership].right_semijoin(
        ctx.from[view_pk_player],
        [](const auto& membership, const auto& player) {
            return membership.player_id.eq(player.id);
        });
}

SPACETIMEDB_VIEW(ViewPkPlayerQuery, sender_view_pk_players_b, Public, ViewContext ctx) {
    return ctx.from[view_pk_membership_secondary].right_semijoin(
        ctx.from[view_pk_player],
        [](const auto& membership, const auto& player) {
            return membership.player_id.eq(player.id);
        });
}

