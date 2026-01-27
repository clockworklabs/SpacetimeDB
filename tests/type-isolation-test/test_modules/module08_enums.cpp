#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)
#include <variant>

using namespace SpacetimeDB;

// Module 8: Enums and enum tables
// Testing if enum types cause WASM issues


// SimpleEnum - basic enum using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// EnumWithPayload - variant enum with payloads using new unified syntax
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

// OneSimpleEnum table
struct OneSimpleEnum { SimpleEnum e; };
SPACETIMEDB_STRUCT(OneSimpleEnum, e)
SPACETIMEDB_TABLE(OneSimpleEnum, one_simple_enum, Public)

// OneEnumWithPayload table
struct OneEnumWithPayload { EnumWithPayload e; };
SPACETIMEDB_STRUCT(OneEnumWithPayload, e)
SPACETIMEDB_TABLE(OneEnumWithPayload, one_enum_with_payload, Public)

// VecSimpleEnum table
struct VecSimpleEnum { std::vector<SimpleEnum> e; };
SPACETIMEDB_STRUCT(VecSimpleEnum, e)
SPACETIMEDB_TABLE(VecSimpleEnum, vec_simple_enum, Public)

// VecEnumWithPayload table
struct VecEnumWithPayload { std::vector<EnumWithPayload> e; };
SPACETIMEDB_STRUCT(VecEnumWithPayload, e)
SPACETIMEDB_TABLE(VecEnumWithPayload, vec_enum_with_payload, Public)

// PkSimpleEnum table
struct PkSimpleEnum { SimpleEnum a; int32_t data; };
SPACETIMEDB_STRUCT(PkSimpleEnum, a, data)
SPACETIMEDB_TABLE(PkSimpleEnum, pk_simple_enum, Public)
FIELD_PrimaryKey(pk_simple_enum, a)

// IndexedSimpleEnum table
struct IndexedSimpleEnum {
    SimpleEnum n;
};
SPACETIMEDB_STRUCT(IndexedSimpleEnum, n)
SPACETIMEDB_TABLE(IndexedSimpleEnum, indexed_simple_enum, Public)
FIELD_Index(indexed_simple_enum, n)

// Reducers for enum types
SPACETIMEDB_REDUCER(insert_one_simple_enum, ReducerContext ctx, SimpleEnum e)
{
    ctx.db.table<OneSimpleEnum>("one_simple_enum").insert(OneSimpleEnum{e});
}

SPACETIMEDB_REDUCER(insert_one_enum_with_payload, ReducerContext ctx, EnumWithPayload e)
{
    ctx.db.table<OneEnumWithPayload>("one_enum_with_payload").insert(OneEnumWithPayload{e});
}