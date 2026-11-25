// Outbox-ness belongs to the table, not the row type. Two outbox tables can
// share a row type, and a regular table sharing it keeps update and delete.
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
SPACETIMEDB_OUTBOX_TABLE(PingOutbox, urgent_ping_outbox, Private, rx::reducers::receive_ping)
FIELD_PrimaryKeyAutoInc(urgent_ping_outbox, msg_id)
FIELD_Target(urgent_ping_outbox, target)
SPACETIMEDB_TABLE(PingOutbox, sent_pings, Private)
FIELD_PrimaryKey(sent_pings, msg_id)

SPACETIMEDB_REDUCER(send, ReducerContext ctx, rx::Identity world, std::string body) {
    PingOutbox queued = ctx.db[ping_outbox].insert(PingOutbox{0, world, body});
    ctx.db[urgent_ping_outbox].insert(PingOutbox{0, world, std::move(body)});

    // Outbox rows can still be read by key.
    if (ctx.db[ping_outbox_msg_id].find(queued.msg_id)) {
        ctx.db[sent_pings].insert(queued);
    }
    // The regular table sharing the row type is not insert-only.
    ctx.db[sent_pings_msg_id].update(queued);
    ctx.db[sent_pings_msg_id].delete_by_key(queued.msg_id);
    return Ok();
}
