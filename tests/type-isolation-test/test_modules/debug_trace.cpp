#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDb;

// Simple table with optional field
struct DebugTable {
    uint32_t id;
    std::optional<int32_t> maybe_value;
};

SPACETIMEDB_STRUCT(DebugTable, id, maybe_value)
SPACETIMEDB_TABLE(DebugTable, debug_table, Public)

