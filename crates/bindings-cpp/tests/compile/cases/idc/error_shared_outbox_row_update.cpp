// A regular table sharing the row type does not make an outbox table updatable.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

struct PingOutbox {
    uint64_t msg_id;
    rx::Identity target;
    std::string body;
};
SPACETIMEDB_STRUCT(PingOutbox, msg_id, target, body)
SPACETIMEDB_OUTBOX_TABLE(PingOutbox, ping_outbox, Private, rx::reducers::receive_ping)
FIELD_PrimaryKeyAutoInc(ping_outbox, msg_id)
FIELD_Target(ping_outbox, target)
SPACETIMEDB_TABLE(PingOutbox, sent_pings, Private)
FIELD_PrimaryKey(sent_pings, msg_id)

SPACETIMEDB_REDUCER(rewrite, ReducerContext ctx, PingOutbox row) {
    ctx.db[ping_outbox_msg_id].update(row);
    return Ok();
}
