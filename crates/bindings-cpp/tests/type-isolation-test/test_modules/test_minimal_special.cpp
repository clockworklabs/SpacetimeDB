#include <spacetimedb.h>

using namespace SpacetimeDB;

// Minimal test - just one table with Identity and one reducer with Identity parameter

// Simple Identity table
struct SimpleIdentityTable { Identity id; };
SPACETIMEDB_STRUCT(SimpleIdentityTable, id)
SPACETIMEDB_TABLE(SimpleIdentityTable, simple_identity_table, Public)

// Single reducer with direct Identity parameter
SPACETIMEDB_REDUCER(test_identity_reducer, ReducerContext ctx, Identity i)
{
    ctx.db.table<SimpleIdentityTable>("simple_identity_table").insert(SimpleIdentityTable{i});
}