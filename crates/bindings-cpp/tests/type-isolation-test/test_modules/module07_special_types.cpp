#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 7: Special types (Identity, ConnectionId, Timestamp)
// Testing if special SpacetimeDB types cause WASM issues


// OneIdentity table
struct OneIdentity { Identity i; };
SPACETIMEDB_STRUCT(OneIdentity, i)
SPACETIMEDB_TABLE(OneIdentity, one_identity, Public)

// OneConnectionId table
struct OneConnectionId { ConnectionId a; };
SPACETIMEDB_STRUCT(OneConnectionId, a)
SPACETIMEDB_TABLE(OneConnectionId, one_connection_id, Public)

// OneTimestamp table
struct OneTimestamp { Timestamp t; };
SPACETIMEDB_STRUCT(OneTimestamp, t)
SPACETIMEDB_TABLE(OneTimestamp, one_timestamp, Public)

// VecIdentity table
struct VecIdentity { std::vector<Identity> i; };
SPACETIMEDB_STRUCT(VecIdentity, i)
SPACETIMEDB_TABLE(VecIdentity, vec_identity, Public)

// VecConnectionId table
struct VecConnectionId { std::vector<ConnectionId> a; };
SPACETIMEDB_STRUCT(VecConnectionId, a)
SPACETIMEDB_TABLE(VecConnectionId, vec_connection_id, Public)

// VecTimestamp table
struct VecTimestamp { std::vector<Timestamp> t; };
SPACETIMEDB_STRUCT(VecTimestamp, t)
SPACETIMEDB_TABLE(VecTimestamp, vec_timestamp, Public)

// UniqueIdentity table
struct UniqueIdentity { Identity i; int32_t data; };
SPACETIMEDB_STRUCT(UniqueIdentity, i, data)
SPACETIMEDB_TABLE(UniqueIdentity, unique_identity, Public)
FIELD_Unique(unique_identity, i)

// UniqueConnectionId table
struct UniqueConnectionId { ConnectionId a; int32_t data; };
SPACETIMEDB_STRUCT(UniqueConnectionId, a, data)
SPACETIMEDB_TABLE(UniqueConnectionId, unique_connection_id, Public)
FIELD_Unique(unique_connection_id, a)

// PkIdentity table
struct PkIdentity { Identity i; int32_t data; };
SPACETIMEDB_STRUCT(PkIdentity, i, data)
SPACETIMEDB_TABLE(PkIdentity, pk_identity, Public)
FIELD_PrimaryKey(pk_identity, i)

// PkConnectionId table
struct PkConnectionId { ConnectionId a; int32_t data; };
SPACETIMEDB_STRUCT(PkConnectionId, a, data)
SPACETIMEDB_TABLE(PkConnectionId, pk_connection_id, Public)
FIELD_PrimaryKey(pk_connection_id, a)

// Users table
struct Users {
    Identity identity;
    std::string name;
};
SPACETIMEDB_STRUCT(Users, identity, name)
SPACETIMEDB_TABLE(Users, users, Public)
FIELD_PrimaryKey(users, identity)

// Parameter wrappers to avoid direct special type parameters (causes WASM traps)
struct IdentityParam { Identity i; };
SPACETIMEDB_STRUCT(IdentityParam, i)

struct ConnectionIdParam { ConnectionId a; };
SPACETIMEDB_STRUCT(ConnectionIdParam, a)

struct TimestampParam { Timestamp t; };
SPACETIMEDB_STRUCT(TimestampParam, t)

// Reducers for special types (using wrapped parameters)
SPACETIMEDB_REDUCER(insert_one_identity, ReducerContext ctx, IdentityParam param)
{
    ctx.db.table<OneIdentity>("one_identity").insert(OneIdentity{param.i});
}

SPACETIMEDB_REDUCER(insert_one_connection_id, ReducerContext ctx, ConnectionIdParam param)
{
    ctx.db.table<OneConnectionId>("one_connection_id").insert(OneConnectionId{param.a});
}

SPACETIMEDB_REDUCER(insert_one_timestamp, ReducerContext ctx, TimestampParam param)
{
    ctx.db.table<OneTimestamp>("one_timestamp").insert(OneTimestamp{param.t});
}

// TEST: Direct special type parameters (should cause WASM trap when published)
SPACETIMEDB_REDUCER(insert_direct_identity, ReducerContext ctx, Identity i)
{
    ctx.db.table<OneIdentity>("one_identity").insert(OneIdentity{i});
}

SPACETIMEDB_REDUCER(insert_direct_connection_id, ReducerContext ctx, ConnectionId c)
{
    ctx.db.table<OneConnectionId>("one_connection_id").insert(OneConnectionId{c});
}

SPACETIMEDB_REDUCER(insert_direct_timestamp, ReducerContext ctx, Timestamp t)
{
    LOG_INFO("Direct timestamp reducer called");
}