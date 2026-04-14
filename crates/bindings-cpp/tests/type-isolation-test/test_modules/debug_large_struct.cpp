#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDB;

// ISOLATION TEST: Large struct causing client codegen issues
// Test if EveryPrimitiveStruct alone causes the "non-special product or sum type" error

// Recreate the problematic large struct
struct EveryPrimitiveStruct {
    uint8_t a;
    uint16_t b;
    uint32_t c;
    uint64_t d;
    u128 e;
    u256 f;
    int8_t g;
    int16_t h;
    int32_t i;
    int64_t j;
    i128 k;
    i256 l;
    bool m;
    float n;
    double o;
    std::string p;
    Identity q;
    ConnectionId r;
    Timestamp s;
    TimeDuration t;
};
SPACETIMEDB_STRUCT(EveryPrimitiveStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

// Simple table using the large struct
struct TestTable { 
    EveryPrimitiveStruct s; 
    int32_t id;
};
SPACETIMEDB_STRUCT(TestTable, s, id)
SPACETIMEDB_TABLE(TestTable, test_table, Public)

// Simple reducer without problematic parameters
SPACETIMEDB_REDUCER(test_basic, ReducerContext ctx)
{
    LOG_INFO("Basic reducer called");
}