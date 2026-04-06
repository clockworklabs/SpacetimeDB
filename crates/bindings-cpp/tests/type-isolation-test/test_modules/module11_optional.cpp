#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)
#include <optional>
#include <variant>

using namespace SpacetimeDB;

// Module 11: Optional types
// Testing if optional types cause WASM issues


// Need enums for optional enum test using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// Need struct for optional struct test
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

// CRITICAL FIX: Add a direct table using EveryPrimitiveStruct to ensure it gets registered by name
// This prevents it from being inlined when used in Options
SPACETIMEDB_TABLE(EveryPrimitiveStruct, every_primitive_direct, Public)

// OptionI32 table
struct OptionI32 { std::optional<int32_t> n; };
SPACETIMEDB_STRUCT(OptionI32, n)
SPACETIMEDB_TABLE(OptionI32, option_i32, Public)

// OptionString table
struct OptionString { std::optional<std::string> s; };
SPACETIMEDB_STRUCT(OptionString, s)
SPACETIMEDB_TABLE(OptionString, option_string, Public)

// OptionIdentity table
struct OptionIdentity { std::optional<Identity> i; };
SPACETIMEDB_STRUCT(OptionIdentity, i)
SPACETIMEDB_TABLE(OptionIdentity, option_identity, Public)

// OptionSimpleEnum table
struct OptionSimpleEnum { std::optional<SimpleEnum> e; };
SPACETIMEDB_STRUCT(OptionSimpleEnum, e)
SPACETIMEDB_TABLE(OptionSimpleEnum, option_simple_enum, Public)

// OptionEveryPrimitiveStruct table
struct OptionEveryPrimitiveStruct { std::optional<EveryPrimitiveStruct> s; };
SPACETIMEDB_STRUCT(OptionEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(OptionEveryPrimitiveStruct, option_every_primitive_struct, Public)

// Complex nested optional type
struct OptionVecOptionI32 { std::optional<std::vector<std::optional<int32_t>>> v; };
SPACETIMEDB_STRUCT(OptionVecOptionI32, v)
SPACETIMEDB_TABLE(OptionVecOptionI32, option_vec_option_i32, Public)

// Parameter wrappers to avoid direct std::optional parameters (causes WASM traps)
struct OptionalI32Param { std::optional<int32_t> n; };
SPACETIMEDB_STRUCT(OptionalI32Param, n)

struct OptionalStringParam { std::optional<std::string> s; };
SPACETIMEDB_STRUCT(OptionalStringParam, s)

struct OptionalIdentityParam { std::optional<Identity> i; };
SPACETIMEDB_STRUCT(OptionalIdentityParam, i)

// Reducers for optional types (using wrapped parameters)
SPACETIMEDB_REDUCER(insert_option_i32, ReducerContext ctx, OptionalI32Param param)
{
    ctx.db.table<OptionI32>("option_i32").insert(OptionI32{param.n});
}

SPACETIMEDB_REDUCER(insert_option_string, ReducerContext ctx, OptionalStringParam param)
{
    ctx.db.table<OptionString>("option_string").insert(OptionString{param.s});
}

SPACETIMEDB_REDUCER(insert_option_identity, ReducerContext ctx, OptionalIdentityParam param)
{
    ctx.db.table<OptionIdentity>("option_identity").insert(OptionIdentity{param.i});
}

// TEST: Direct optional parameters (should cause WASM trap when published)
SPACETIMEDB_REDUCER(insert_direct_option_i32, ReducerContext ctx, std::optional<int32_t> n)
{
    ctx.db.table<OptionI32>("option_i32").insert(OptionI32{n});
}

SPACETIMEDB_REDUCER(insert_direct_option_string, ReducerContext ctx, std::optional<std::string> s)
{
    ctx.db.table<OptionString>("option_string").insert(OptionString{s});
}

SPACETIMEDB_REDUCER(insert_direct_option_every_primitive_struct, ReducerContext ctx, std::optional<EveryPrimitiveStruct> s)
{
    ctx.db.table<OptionEveryPrimitiveStruct>("option_every_primitive_struct").insert(OptionEveryPrimitiveStruct{s});
}