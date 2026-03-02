#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;


// Simple table with just Timestamp
struct TestTimestamp {
    uint32_t id;
    Timestamp timestamp;
};
SPACETIMEDB_STRUCT(TestTimestamp, id, timestamp)
SPACETIMEDB_TABLE(TestTimestamp, test_timestamp, Public)