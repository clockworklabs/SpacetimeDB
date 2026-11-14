// An identity meant for one receiver module can't be used for another.
#include <spacetimedb.h>
#include <receiver/bindings.h>
#include <other/bindings.h>

using namespace SpacetimeDB;

SPACETIMEDB_REDUCER(mix_up, ReducerContext ctx, other::Identity target) {
    receiver::Identity world = target;
    (void)world;
    (void)ctx;
    return Ok();
}
