#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test: Single vector of special type to isolate the exact error
// Start with just std::vector<Identity> to pinpoint the issue


// Test just one vector of Identity
struct TestVecIdentity { std::vector<Identity> identities; };
SPACETIMEDB_STRUCT(TestVecIdentity, identities)
SPACETIMEDB_TABLE(TestVecIdentity, test_vec_identity, Public)