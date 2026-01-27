#include <spacetimedb.h>

using namespace SpacetimeDB;

// Minimal test for debugging special type vectors


// Single table with vector of Identity
struct DebugIdentityVec { 
    std::vector<Identity> ids; 
};
SPACETIMEDB_STRUCT(DebugIdentityVec, ids)
SPACETIMEDB_TABLE(DebugIdentityVec, debug_identity_vec, Public)