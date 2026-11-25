// The receiver must be a generated reducer handle.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

struct NotAReducer {};
struct PingOutbox {
    uint64_t msg_id;
    rx::Identity target;
};
SPACETIMEDB_STRUCT(PingOutbox, msg_id, target)
SPACETIMEDB_OUTBOX_TABLE(PingOutbox, ping_outbox, Private, NotAReducer)
