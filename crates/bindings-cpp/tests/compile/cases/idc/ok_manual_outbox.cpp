// Manual outbox declarations: aliases, reordered columns, bookkeeping columns,
// indexes, a no-argument receiver, and on_result reducers.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

// Columns in a different order from the receiver's parameters, with an alias,
// an extra bookkeeping column, and an index on the target.
struct DetailedPingOutbox {
    rx::PingDetails details;
    Timestamp requested_at;
    uint64_t id;
    std::string message;
    rx::Identity world;
};
SPACETIMEDB_STRUCT(DetailedPingOutbox, details, requested_at, id, message, world)
SPACETIMEDB_OUTBOX_TABLE(DetailedPingOutbox, detailed_ping_outbox, Public, rx::reducers::receive_detailed_ping)
FIELD_PrimaryKeyAutoInc(detailed_ping_outbox, id)
FIELD_Target(detailed_ping_outbox, world)
FIELD_Index(detailed_ping_outbox, world)
FIELD_Param(detailed_ping_outbox, message, body)

struct NoopOutbox {
    uint64_t msg_id;
    rx::Identity target;
};
SPACETIMEDB_STRUCT(NoopOutbox, msg_id, target)
SPACETIMEDB_OUTBOX_TABLE(NoopOutbox, noop_outbox, Private, rx::reducers::noop, "noop_calls")
FIELD_PrimaryKeyAutoInc(noop_outbox, msg_id)
FIELD_Target(noop_outbox, target)

struct DetailedPingResult {
    uint64_t id;
    bool succeeded;
};
SPACETIMEDB_STRUCT(DetailedPingResult, id, succeeded)
SPACETIMEDB_TABLE(DetailedPingResult, detailed_ping_result, Private)
FIELD_PrimaryKey(detailed_ping_result, id)

SPACETIMEDB_REDUCER(send, ReducerContext ctx, rx::Identity world, std::string message) {
    DetailedPingOutbox row{rx::PingDetails{}, ctx.timestamp, 0, std::move(message), world};
    DetailedPingOutbox inserted = ctx.db[detailed_ping_outbox].insert(row);
    LOG_INFO("queued call " + std::to_string(inserted.id));
    ctx.db[noop_outbox].insert(NoopOutbox{0, world});

    // Outbox rows can be read.
    uint64_t pending = ctx.db[detailed_ping_outbox].count();
    for (const auto& queued : ctx.db[detailed_ping_outbox]) {
        (void)queued;
    }
    for (const auto& queued : ctx.db[detailed_ping_outbox_world].filter(world)) {
        (void)queued;
    }
    auto found = ctx.db[detailed_ping_outbox_id].find(inserted.id);
    (void)found;
    (void)pending;
    return Ok();
}

SPACETIMEDB_REDUCER_ON_RESULT(detailed_ping_outbox, on_detailed_ping_result,
                              ReducerContext ctx, DetailedPingOutbox row, OutboxResult result) {
    ctx.db[detailed_ping_result].insert(DetailedPingResult{row.id, result.is_ok()});
    if (result.is_err()) {
        LOG_WARN("call failed: " + result.unwrap_err());
    }
    return Ok();
}
