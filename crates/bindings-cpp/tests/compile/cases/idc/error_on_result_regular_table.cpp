// on_result requires an outbox table.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;
namespace rx = receiver;

struct Peer {
    uint64_t id;
};
SPACETIMEDB_STRUCT(Peer, id)
SPACETIMEDB_TABLE(Peer, peer, Private)
SPACETIMEDB_REDUCER_ON_RESULT(peer, on_peer_result, ReducerContext ctx, Peer row, OutboxResult result) {
    return Ok();
}
