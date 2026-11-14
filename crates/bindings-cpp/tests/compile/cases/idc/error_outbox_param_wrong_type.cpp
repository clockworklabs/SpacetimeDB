// An aliased column must have the receiver parameter's type.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

struct PingOutbox {
    uint64_t msg_id;
    rx::Identity target;
    uint32_t message;
};
SPACETIMEDB_STRUCT(PingOutbox, msg_id, target, message)
SPACETIMEDB_OUTBOX_TABLE(PingOutbox, ping_outbox, Private, rx::reducers::receive_ping)
FIELD_PrimaryKeyAutoInc(ping_outbox, msg_id)
FIELD_Target(ping_outbox, target)
FIELD_Param(ping_outbox, message, body)
