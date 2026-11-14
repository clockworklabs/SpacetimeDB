// A plain Identity converts to a receiver's identity type only explicitly.
#include <spacetimedb.h>
#include <receiver/bindings.h>

using namespace SpacetimeDB;

SPACETIMEDB_REDUCER(implicit, ReducerContext ctx) {
    receiver::Identity world = ctx.sender();
    (void)world;
    return Ok();
}
