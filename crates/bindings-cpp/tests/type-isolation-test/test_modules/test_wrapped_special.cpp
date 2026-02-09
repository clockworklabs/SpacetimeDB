#include <spacetimedb.h>

using namespace SpacetimeDB;

// Control test - same functionality but with wrapped parameters (should work)

// Simple Identity table
struct SimpleIdentityTable { Identity id; };
SPACETIMEDB_STRUCT(SimpleIdentityTable, id)
SPACETIMEDB_TABLE(SimpleIdentityTable, simple_identity_table, Public)

// Parameter wrapper struct
struct IdentityParam { Identity id; };
SPACETIMEDB_STRUCT(IdentityParam, id)

// Reducer with wrapped Identity parameter (should work)
SPACETIMEDB_REDUCER(test_identity_reducer, ReducerContext ctx, IdentityParam param)
{
    ctx.db.table<SimpleIdentityTable>("simple_identity_table").insert(SimpleIdentityTable{param.id});
}