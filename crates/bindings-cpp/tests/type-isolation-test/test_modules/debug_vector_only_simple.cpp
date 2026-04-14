#include <spacetimedb.h>

using namespace SpacetimeDB;


// SimpleEnum defined but ONLY used in vector context using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// Variant enum with vector<SimpleEnum> - this triggers the problem using new unified syntax
SPACETIMEDB_ENUM(VectorEnum,
    (SimpleEnums, std::vector<SimpleEnum>)
)

// Table using VectorEnum (which contains vector<SimpleEnum>)
struct VectorTable { VectorEnum ve; };
SPACETIMEDB_STRUCT(VectorTable, ve)
SPACETIMEDB_TABLE(VectorTable, vector_table, Public)

// NO direct SimpleEnum usage - only through vector