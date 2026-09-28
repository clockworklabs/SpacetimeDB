
#include "spacetimedb.h"

using namespace SpacetimeDB;

struct DefaultsTestTable {
    uint32_t id;
};
SPACETIMEDB_STRUCT(DefaultsTestTable, id)
SPACETIMEDB_TABLE(DefaultsTestTable, defaults_test_table, Public)
