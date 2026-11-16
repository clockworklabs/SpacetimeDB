// Outbox rows can't be deleted through a multi-column index either.
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
FIELD_MultiColumnIndex(ping_outbox, by_target_body, target, body)

SPACETIMEDB_REDUCER(drop_pings, ReducerContext ctx, PingOutbox row) {
    ctx.db[ping_outbox_by_target_body].delete_by_value(row);
    return Ok();
}
