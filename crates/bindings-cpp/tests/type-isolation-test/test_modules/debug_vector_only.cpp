#include <spacetimedb.h>
#include <vector>

using namespace SpacetimeDB;

// Minimal test - just one vector variant to see how it's serialized

SPACETIMEDB_ENUM(SimpleVectorEnum,
    (Bytes, std::vector<uint8_t>)
)

struct VectorTable { 
    SimpleVectorEnum e; 
};
SPACETIMEDB_STRUCT(VectorTable, e)
SPACETIMEDB_TABLE(VectorTable, vector_table, Public)

SPACETIMEDB_REDUCER(insert_test, ReducerContext ctx)
{
    std::vector<uint8_t> bytes = {1, 2, 3};
    SimpleVectorEnum e = bytes;
    ctx.db.table<VectorTable>("vector_table").insert(VectorTable{e});
}