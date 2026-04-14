#include <spacetimedb.h>

using namespace SpacetimeDB;

// Minimal test: Just the three basic special types together
// Testing if combination of Identity + ConnectionId + Timestamp causes issues


// Test 1: Individual tables (should work based on isolated tests)
struct TestIdentity { Identity i; };
SPACETIMEDB_STRUCT(TestIdentity, i)
SPACETIMEDB_TABLE(TestIdentity, test_identity, Public)

struct TestConnectionId { ConnectionId c; };  
SPACETIMEDB_STRUCT(TestConnectionId, c)
SPACETIMEDB_TABLE(TestConnectionId, test_connection_id, Public)

struct TestTimestamp { Timestamp t; };
SPACETIMEDB_STRUCT(TestTimestamp, t)  
SPACETIMEDB_TABLE(TestTimestamp, test_timestamp, Public)