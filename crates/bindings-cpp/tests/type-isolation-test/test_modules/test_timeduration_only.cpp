#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;


// Simple table with just TimeDuration
struct TestTimeDuration {
    uint32_t id;
    TimeDuration duration;
};
SPACETIMEDB_STRUCT(TestTimeDuration, id, duration)
SPACETIMEDB_TABLE(TestTimeDuration, test_time_duration, Public)