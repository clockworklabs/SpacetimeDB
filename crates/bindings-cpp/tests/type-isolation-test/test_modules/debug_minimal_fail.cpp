#include <spacetimedb.h>

using namespace SpacetimeDb;


// The problematic pattern: SimpleEnum used in multiple contexts using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// Direct table using SimpleEnum
struct DirectTable { SimpleEnum e; };
SPACETIMEDB_STRUCT(DirectTable, e)
SPACETIMEDB_TABLE(DirectTable, direct_table, Public)

// NO reducers - just the table registration conflict