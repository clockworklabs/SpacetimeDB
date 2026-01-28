#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;


// Simple table with just Identity
struct TestIdentity {
    uint32_t id;
    Identity identity;
};
SPACETIMEDB_STRUCT(TestIdentity, id, identity)
SPACETIMEDB_TABLE(TestIdentity, test_identity, Public)