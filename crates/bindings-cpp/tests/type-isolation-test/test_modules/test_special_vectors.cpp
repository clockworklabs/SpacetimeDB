#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test: Vectors of special types
// Testing if std::vector<Identity>, std::vector<ConnectionId>, std::vector<Timestamp> cause issues


// Test vectors of special types
struct VecIdentity { std::vector<Identity> identities; };
SPACETIMEDB_STRUCT(VecIdentity, identities)
SPACETIMEDB_TABLE(VecIdentity, vec_identity, Public)

struct VecConnectionId { std::vector<ConnectionId> connections; };
SPACETIMEDB_STRUCT(VecConnectionId, connections)
SPACETIMEDB_TABLE(VecConnectionId, vec_connection_id, Public)

struct VecTimestamp { std::vector<Timestamp> timestamps; };
SPACETIMEDB_STRUCT(VecTimestamp, timestamps)
SPACETIMEDB_TABLE(VecTimestamp, vec_timestamp, Public)