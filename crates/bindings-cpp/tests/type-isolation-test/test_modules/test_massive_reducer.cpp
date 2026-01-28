#include <spacetimedb.h>

using namespace SpacetimeDB;

// Recreate the problematic pattern from lib.cpp
SPACETIMEDB_ENUM(SimpleEnum, A, B, C)

SPACETIMEDB_ENUM(EnumWithPayload,
    (U8, uint8_t),
    (U16, uint16_t),
    (U32, uint32_t),
    (U64, uint64_t),
    (U128, SpacetimeDB::u128),
    (U256, SpacetimeDB::u256),
    (I8, int8_t),
    (I16, int16_t),
    (I32, int32_t),
    (I64, int64_t),
    (I128, SpacetimeDB::i128),
    (I256, SpacetimeDB::i256),
    (Bool, bool),
    (F32, float),
    (F64, double),
    (Str, std::string),
    (Identity, SpacetimeDB::Identity),
    (ConnectionId, SpacetimeDB::ConnectionId),
    (Timestamp, SpacetimeDB::Timestamp),
    (Bytes, std::vector<uint8_t>),
    (Ints, std::vector<int32_t>),
    (Strings, std::vector<std::string>),
    (SimpleEnums, std::vector<SimpleEnum>)
)

SPACETIMEDB_UNIT_STRUCT(UnitStruct)

struct ByteStruct { uint8_t b; };
SPACETIMEDB_STRUCT(ByteStruct, b)

struct EveryPrimitiveStruct {
    uint8_t a; uint16_t b; uint32_t c; uint64_t d; u128 e; u256 f;
    int8_t g; int16_t h; int32_t i; int64_t j; i128 k; i256 l;
    bool m; float n; double o; std::string p;
    Identity q; ConnectionId r; Timestamp s; TimeDuration t;
};
SPACETIMEDB_STRUCT(EveryPrimitiveStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

struct EveryVecStruct {
    std::vector<uint8_t> a; std::vector<uint16_t> b; std::vector<uint32_t> c; std::vector<uint64_t> d;
    std::vector<u128> e; std::vector<u256> f; std::vector<int8_t> g; std::vector<int16_t> h;
    std::vector<int32_t> i; std::vector<int64_t> j; std::vector<i128> k; std::vector<i256> l;
    std::vector<bool> m; std::vector<float> n; std::vector<double> o; std::vector<std::string> p;
    std::vector<Identity> q; std::vector<ConnectionId> r; std::vector<Timestamp> s; std::vector<TimeDuration> t;
};
SPACETIMEDB_STRUCT(EveryVecStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

// The monster struct that combines everything
struct LargeTable {
    uint8_t a; uint16_t b; uint32_t c; uint64_t d; u128 e; u256 f;
    int8_t g; int16_t h; int32_t i; int64_t j; i128 k; i256 l;
    bool m; float n; double o; std::string p;
    SimpleEnum q; EnumWithPayload r; UnitStruct s; ByteStruct t;
    EveryPrimitiveStruct u; EveryVecStruct v;
};
SPACETIMEDB_STRUCT(LargeTable, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v)
SPACETIMEDB_TABLE(LargeTable, large_table, Public)

// The problematic reducer with 22 parameters including complex nested structures
SPACETIMEDB_REDUCER(insert_large_table, ReducerContext ctx,
    uint8_t a, uint16_t b, uint32_t c, uint64_t d, u128 e, u256 f,
    int8_t g, int16_t h, int32_t i, int64_t j, i128 k, i256 l,
    bool m, float n, double o, std::string p,
    SimpleEnum q, EnumWithPayload r, UnitStruct s, ByteStruct t,
    EveryPrimitiveStruct u, EveryVecStruct v)
{
    ctx.db[large_table].insert(LargeTable{
        a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v
    });
}