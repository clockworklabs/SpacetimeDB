#include <spacetimedb.h>

using namespace SpacetimeDB;

// ISOLATION TEST: Test direct special type reducer parameters
// Based on working debug_special_constraints, add direct special type parameters

// Working baseline: Simple special type tables (we know these work)
struct TestIdentity { Identity i; };
SPACETIMEDB_STRUCT(TestIdentity, i)
SPACETIMEDB_TABLE(TestIdentity, test_identity, Public)

struct TestConnectionId { ConnectionId c; };
SPACETIMEDB_STRUCT(TestConnectionId, c)  
SPACETIMEDB_TABLE(TestConnectionId, test_connection_id, Public)

// Parameter wrappers to avoid direct special type parameters (causes WASM traps)
struct IdentityParam { Identity i; };
SPACETIMEDB_STRUCT(IdentityParam, i)

struct ConnectionIdParam { ConnectionId c; };
SPACETIMEDB_STRUCT(ConnectionIdParam, c)

// TEST: Use wrapped special type parameters to avoid WASM trap
SPACETIMEDB_REDUCER(insert_identity, ReducerContext ctx, IdentityParam param)
{
    ctx.db.table<TestIdentity>("test_identity").insert(TestIdentity{param.i});
}

SPACETIMEDB_REDUCER(insert_connection_id, ReducerContext ctx, ConnectionIdParam param)
{
    ctx.db.table<TestConnectionId>("test_connection_id").insert(TestConnectionId{param.c});
}

// Control: basic reducer without special type parameters (known to work)
SPACETIMEDB_REDUCER(test_basic, ReducerContext ctx)
{
    LOG_INFO("Basic reducer called");
}

// TEST: Direct special type parameters (expected to cause WASM trap)
SPACETIMEDB_REDUCER(insert_direct_identity, ReducerContext ctx, Identity i)
{
    ctx.db.table<TestIdentity>("test_identity").insert(TestIdentity{i});
}

SPACETIMEDB_REDUCER(insert_direct_connection_id, ReducerContext ctx, ConnectionId c)
{
    ctx.db.table<TestConnectionId>("test_connection_id").insert(TestConnectionId{c});
}

SPACETIMEDB_REDUCER(insert_direct_timestamp, ReducerContext ctx, Timestamp t)
{
    LOG_INFO("Received timestamp parameter");
}

SPACETIMEDB_REDUCER(insert_direct_time_duration, ReducerContext ctx, TimeDuration d)
{
    LOG_INFO("Received duration parameter");
}