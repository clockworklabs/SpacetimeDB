// FIELD_Target applies only to outbox tables.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

struct Peer {
    uint64_t id;
    rx::Identity target;
};
SPACETIMEDB_STRUCT(Peer, id, target)
SPACETIMEDB_TABLE(Peer, peer, Private)
FIELD_Target(peer, target)
