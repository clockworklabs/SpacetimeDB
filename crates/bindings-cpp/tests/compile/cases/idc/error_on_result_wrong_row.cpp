// on_result takes the outbox's own row type.
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

struct OtherRow {
    uint64_t id;
};
SPACETIMEDB_STRUCT(OtherRow, id)
SPACETIMEDB_REDUCER_ON_RESULT(ping_outbox, on_ping_result, ReducerContext ctx, OtherRow row, OutboxResult result) {
    return Ok();
}
