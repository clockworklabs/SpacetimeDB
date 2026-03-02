#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)
#include <variant>

using namespace SpacetimeDB;

// Module 9: Structs and complex types
// Testing if struct types cause WASM issues


// Need enums for struct fields using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)
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

// UnitStruct
struct UnitStruct {
    uint8_t dummy = 0;
};
SPACETIMEDB_STRUCT(UnitStruct, dummy)

// ByteStruct
struct ByteStruct {
    uint8_t b;
};
SPACETIMEDB_STRUCT(ByteStruct, b)

// EveryPrimitiveStruct
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

// EveryVecStruct
struct EveryVecStruct {
    std::vector<uint8_t> a;
    std::vector<uint16_t> b;
    std::vector<uint32_t> c;
    std::vector<uint64_t> d;
    std::vector<u128> e;
    std::vector<u256> f;
    std::vector<int8_t> g;
    std::vector<int16_t> h;
    std::vector<int32_t> i;
    std::vector<int64_t> j;
    std::vector<i128> k;
    std::vector<i256> l;
    std::vector<bool> m;
    std::vector<float> n;
    std::vector<double> o;
    std::vector<std::string> p;
    std::vector<Identity> q;
    std::vector<ConnectionId> r;
    std::vector<Timestamp> s;
    std::vector<TimeDuration> t;
    std::vector<SimpleEnum> u;
};
SPACETIMEDB_STRUCT(EveryVecStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u)

// OneUnitStruct table
struct OneUnitStruct { UnitStruct s; };
SPACETIMEDB_STRUCT(OneUnitStruct, s)
SPACETIMEDB_TABLE(OneUnitStruct, one_unit_struct, Public)

// OneByteStruct table
struct OneByteStruct { ByteStruct s; };
SPACETIMEDB_STRUCT(OneByteStruct, s)
SPACETIMEDB_TABLE(OneByteStruct, one_byte_struct, Public)

// OneEveryPrimitiveStruct table
struct OneEveryPrimitiveStruct { EveryPrimitiveStruct s; };
SPACETIMEDB_STRUCT(OneEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(OneEveryPrimitiveStruct, one_every_primitive_struct, Public)

// OneEveryVecStruct table
struct OneEveryVecStruct { EveryVecStruct s; };
SPACETIMEDB_STRUCT(OneEveryVecStruct, s)
SPACETIMEDB_TABLE(OneEveryVecStruct, one_every_vec_struct, Public)

// VecUnitStruct table
struct VecUnitStruct { std::vector<UnitStruct> s; };
SPACETIMEDB_STRUCT(VecUnitStruct, s)
SPACETIMEDB_TABLE(VecUnitStruct, vec_unit_struct, Public)

// VecByteStruct table
struct VecByteStruct { std::vector<ByteStruct> s; };
SPACETIMEDB_STRUCT(VecByteStruct, s)
SPACETIMEDB_TABLE(VecByteStruct, vec_byte_struct, Public)

// VecEveryPrimitiveStruct table
struct VecEveryPrimitiveStruct { std::vector<EveryPrimitiveStruct> s; };
SPACETIMEDB_STRUCT(VecEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(VecEveryPrimitiveStruct, vec_every_primitive_struct, Public)

// VecEveryVecStruct table
struct VecEveryVecStruct { std::vector<EveryVecStruct> s; };
SPACETIMEDB_STRUCT(VecEveryVecStruct, s)
SPACETIMEDB_TABLE(VecEveryVecStruct, vec_every_vec_struct, Public)

// LargeTable
struct LargeTable {
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
    SimpleEnum q;
    EnumWithPayload r;
    UnitStruct s;
    ByteStruct t;
    EveryPrimitiveStruct u;
    EveryVecStruct v;
};
SPACETIMEDB_STRUCT(LargeTable, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v)
SPACETIMEDB_TABLE(LargeTable, large_table, Public)

// Reducers for struct types
SPACETIMEDB_REDUCER(insert_one_unit_struct, ReducerContext ctx, UnitStruct s)
{
    ctx.db.table<OneUnitStruct>("one_unit_struct").insert(OneUnitStruct{s});
}

SPACETIMEDB_REDUCER(insert_one_byte_struct, ReducerContext ctx, ByteStruct s)
{
    ctx.db.table<OneByteStruct>("one_byte_struct").insert(OneByteStruct{s});
}

SPACETIMEDB_REDUCER(insert_one_every_primitive_struct, ReducerContext ctx, EveryPrimitiveStruct s)
{
    ctx.db.table<OneEveryPrimitiveStruct>("one_every_primitive_struct").insert(OneEveryPrimitiveStruct{s});
}

SPACETIMEDB_REDUCER(insert_one_every_vec_struct, ReducerContext ctx, EveryVecStruct s)
{
    ctx.db.table<OneEveryVecStruct>("one_every_vec_struct").insert(OneEveryVecStruct{s});
}