// Rows of a generated outbox can't be updated either.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

SPACETIMEDB_OUTBOX_FOR(rx::reducers::receive_ping, ping_outbox)

SPACETIMEDB_REDUCER(rewrite, ReducerContext ctx, rx::outbox::receive_ping row) {
    ctx.db[ping_outbox_msg_id].update(row);
    return Ok();
}
