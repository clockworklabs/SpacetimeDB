#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test the problematic nested optional patterns from lib.cpp

// Large struct to use in optional
struct EveryPrimitiveStruct {
    uint8_t a; uint16_t b; uint32_t c; uint64_t d; u128 e; u256 f;
    int8_t g; int16_t h; int32_t i; int64_t j; i128 k; i256 l;
    bool m; float n; double o; std::string p;
    Identity q; ConnectionId r; Timestamp s; TimeDuration t;
};
SPACETIMEDB_STRUCT(EveryPrimitiveStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

// The problematic nested optional: optional vector of optional integers
struct OptionVecOptionI32 { 
    std::optional<std::vector<std::optional<int32_t>>> v; 
};
SPACETIMEDB_STRUCT(OptionVecOptionI32, v)
SPACETIMEDB_TABLE(OptionVecOptionI32, option_vec_option_i32, Public)

// Optional complex struct
struct OptionEveryPrimitiveStruct { 
    std::optional<EveryPrimitiveStruct> s; 
};
SPACETIMEDB_STRUCT(OptionEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(OptionEveryPrimitiveStruct, option_every_primitive_struct, Public)

// Test reducers with nested optional parameters
SPACETIMEDB_REDUCER(insert_option_vec_option_i32, ReducerContext ctx, std::optional<std::vector<std::optional<int32_t>>> v)
{
    ctx.db[option_vec_option_i32].insert(OptionVecOptionI32{v});
}

SPACETIMEDB_REDUCER(insert_option_every_primitive_struct, ReducerContext ctx, std::optional<EveryPrimitiveStruct> s)
{
    ctx.db[option_every_primitive_struct].insert(OptionEveryPrimitiveStruct{s});
}