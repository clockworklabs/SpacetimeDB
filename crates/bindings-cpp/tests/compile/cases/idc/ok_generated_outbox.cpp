// SPACETIMEDB_OUTBOX_FOR declares outboxes with the bindings' generated row types,
// including for two dependencies with the same reducer, and two tables sharing a row type.
#include <spacetimedb.h>
#include <receiver/bindings.h>
#include <other/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

SPACETIMEDB_OUTBOX_FOR(rx::reducers::receive_ping, ping_outbox)
SPACETIMEDB_OUTBOX_FOR(rx::reducers::receive_ping, urgent_ping_outbox, "urgent_pings")
SPACETIMEDB_OUTBOX_FOR(other::reducers::receive_ping, other_ping_outbox)
SPACETIMEDB_OUTBOX_FOR(rx::reducers::noop, noop_outbox)

using PingRow = rx::reducers::receive_ping::outbox_row;
static_assert(std::is_same_v<PingRow, rx::outbox::receive_ping>);
static_assert(!std::is_same_v<PingRow, other::reducers::receive_ping::outbox_row>);

struct PingResult {
    uint64_t msg_id;
    std::string body;
    bool succeeded;
};
SPACETIMEDB_STRUCT(PingResult, msg_id, body, succeeded)
SPACETIMEDB_TABLE(PingResult, ping_result, Private)

SPACETIMEDB_REDUCER(send, ReducerContext ctx, rx::Identity world, other::Identity peer, std::string body) {
    PingRow inserted = ctx.db[ping_outbox].insert(PingRow{0, world, body});
    ctx.db[urgent_ping_outbox].insert(PingRow{0, world, body});
    ctx.db[other_ping_outbox].insert(other::outbox::receive_ping{0, peer, body});
    ctx.db[noop_outbox].insert(rx::outbox::noop{0, world});
    auto found = ctx.db[ping_outbox_msg_id].find(inserted.msg_id);
    (void)found;
    return Ok();
}

SPACETIMEDB_REDUCER_ON_RESULT(ping_outbox, on_ping_result,
                              ReducerContext ctx, PingRow row, OutboxResult result) {
    ctx.db[ping_result].insert(PingResult{row.msg_id, row.body, result.is_ok()});
    return Ok();
}
