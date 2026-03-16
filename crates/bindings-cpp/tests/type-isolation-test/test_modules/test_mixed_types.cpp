#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;


// Table with working type (Identity)
struct TestIdentity {
    uint32_t id;
    Identity identity;
};
SPACETIMEDB_STRUCT(TestIdentity, id, identity)
SPACETIMEDB_TABLE(TestIdentity, test_identity, Public)

// Table with failing type (u128)  
struct TestU128 {
    uint32_t id;
    u128 value;
};
SPACETIMEDB_STRUCT(TestU128, id, value)
SPACETIMEDB_TABLE(TestU128, test_u128, Public)