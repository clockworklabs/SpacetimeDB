#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;


// Simple table with just ConnectionId
struct TestConnectionId {
    uint32_t id;
    ConnectionId conn_id;
};
SPACETIMEDB_STRUCT(TestConnectionId, id, conn_id)
SPACETIMEDB_TABLE(TestConnectionId, test_connection_id, Public)