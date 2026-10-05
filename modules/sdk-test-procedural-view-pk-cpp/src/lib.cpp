#include <spacetimedb.h>

using namespace SpacetimeDB;

struct LeftSource {
    uint64_t id;
    Identity sender;
    uint64_t filter;
};
SPACETIMEDB_STRUCT(LeftSource, id, sender, filter)
SPACETIMEDB_TABLE(LeftSource, left_source, Public)
FIELD_PrimaryKey(left_source, id)
FIELD_Index(left_source, sender)

struct RightSource {
    uint64_t id;
    Identity sender;
    uint64_t filter;
};
SPACETIMEDB_STRUCT(RightSource, id, sender, filter)
SPACETIMEDB_TABLE(RightSource, right_source, Public)
FIELD_PrimaryKey(right_source, id)
FIELD_Index(right_source, sender)

SPACETIMEDB_REDUCER(insert_left, ReducerContext ctx, uint64_t id, uint64_t filter) {
    ctx.db[left_source].insert(LeftSource{id, ctx.sender(), filter});
    return Ok();
}

SPACETIMEDB_REDUCER(update_left, ReducerContext ctx, uint64_t id, uint64_t filter) {
    ctx.db[left_source_id].update(LeftSource{id, ctx.sender(), filter});
    return Ok();
}

SPACETIMEDB_REDUCER(insert_right, ReducerContext ctx, uint64_t id, uint64_t filter) {
    ctx.db[right_source].insert(RightSource{id, ctx.sender(), filter});
    return Ok();
}

SPACETIMEDB_VIEW(std::vector<LeftSource>, sender_left_view, Public, ViewContext ctx) {
    return ctx.db[left_source_sender].filter(ctx.sender()).collect();
}
VIEW_PrimaryKey(sender_left_view, id)

SPACETIMEDB_VIEW(std::vector<RightSource>, sender_right_view, Public, ViewContext ctx) {
    return ctx.db[right_source_sender].filter(ctx.sender()).collect();
}
VIEW_PrimaryKey(sender_right_view, id)
