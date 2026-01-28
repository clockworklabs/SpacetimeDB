#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDB;

// ISOLATION TEST: std::optional<LargeStruct> causing client codegen issues
// Testing if optional wrapper around large struct causes the error

// The large struct (we know this alone works)
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

// TEST: Optional wrapper around large struct - does this cause client codegen error?
struct OptionLargeStruct { 
    std::optional<EveryPrimitiveStruct> s; 
};
SPACETIMEDB_STRUCT(OptionLargeStruct, s)
SPACETIMEDB_TABLE(OptionLargeStruct, option_large_struct, Public)

// Simple reducer without problematic parameters
SPACETIMEDB_REDUCER(test_basic, ReducerContext ctx)
{
    LOG_INFO("Basic reducer called");
}