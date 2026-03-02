#include <spacetimedb.h>

using namespace SpacetimeDB;

// ISOLATION TEST: Test constraints on special types
// Based on working test_special_minimal, add constraints to see if they cause WASM traps

// Working baseline: Simple special type table
struct TestIdentity { Identity i; };
SPACETIMEDB_STRUCT(TestIdentity, i)
SPACETIMEDB_TABLE(TestIdentity, test_identity, Public)

// TEST: Add Unique constraint on Identity - does this cause WASM trap?
struct UniqueIdentity { Identity i; int32_t data; };
SPACETIMEDB_STRUCT(UniqueIdentity, i, data)
SPACETIMEDB_TABLE(UniqueIdentity, unique_identity, Public)
FIELD_Unique(unique_identity, i)

// TEST: Add PrimaryKey constraint on Identity - does this cause WASM trap?
struct PkIdentity { Identity i; int32_t data; };
SPACETIMEDB_STRUCT(PkIdentity, i, data)  
SPACETIMEDB_TABLE(PkIdentity, pk_identity, Public)
FIELD_PrimaryKey(pk_identity, i)

// Simple reducer without special type parameters
SPACETIMEDB_REDUCER(test_basic, ReducerContext ctx)
{
    LOG_INFO("Basic reducer called");
}