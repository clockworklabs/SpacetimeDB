#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)
#include <optional>
#include <cstdio>

using namespace SpacetimeDB;

// Global constructor to run before __preinit__
struct DebugInit {
    DebugInit() {
        fprintf(stdout, "DEBUG: DebugInit constructor called\n");
    }
};
static DebugInit debug_init;


// Table with optional field - will this trigger the crash?
struct OptionalTable {
    uint32_t id;
    std::optional<int32_t> maybe_value;
};

// First register the BSATN struct
SPACETIMEDB_STRUCT(OptionalTable, id, maybe_value)

// Global constructor after BSATN registration
struct AfterBsatn {
    AfterBsatn() {
        fprintf(stdout, "DEBUG: AfterBsatn constructor called\n");
    }
};
static AfterBsatn after_bsatn;

// Then register the table
SPACETIMEDB_TABLE(OptionalTable, optional_table, Public)

// Global constructor after TABLE registration
struct AfterTable {
    AfterTable() {
        fprintf(stdout, "DEBUG: AfterTable constructor called\n");
    }
};
static AfterTable after_table;