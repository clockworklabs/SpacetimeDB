#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;


// Simple table with just u128
struct TestU128 {
    uint32_t id;
    u128 value;
};
SPACETIMEDB_STRUCT(TestU128, id, value)
SPACETIMEDB_TABLE(TestU128, test_u128, Public)