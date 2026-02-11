#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)
#include <variant>

using namespace SpacetimeDB;

// Test: EnumWithPayload in table but NOT as reducer parameter
// This isolates whether the issue is with table storage or reducer params


// SimpleEnum - basic enum using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// EnumWithPayload - with all the problematic vector types using new unified syntax
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

// Table with EnumWithPayload - this should work fine
struct EnumTable { 
    EnumWithPayload e; 
    int32_t id;
};
SPACETIMEDB_STRUCT(EnumTable, e, id)
SPACETIMEDB_TABLE(EnumTable, enum_table, Public)

// Reducers that DON'T use EnumWithPayload as parameters
// Instead use simple types only

SPACETIMEDB_REDUCER(insert_simple, ReducerContext ctx, int32_t id)
{
    // Create an EnumWithPayload and insert it
    EnumWithPayload e = uint8_t{42}; // U8 variant
    ctx.db.table<EnumTable>("enum_table").insert(EnumTable{e, id});
}

SPACETIMEDB_REDUCER(insert_bytes, ReducerContext ctx, int32_t id)
{
    // Create Bytes variant and insert
    std::vector<uint8_t> bytes = {1, 2, 3, 4};
    EnumWithPayload e = bytes; // Bytes variant  
    ctx.db.table<EnumTable>("enum_table").insert(EnumTable{e, id});
}

SPACETIMEDB_REDUCER(query_all, ReducerContext ctx)
{
    auto table = ctx.db.table<EnumTable>("enum_table");
    for (auto& row : table) {
        // Process the EnumWithPayload in table storage
        LOG_INFO("Found enum table row");
    }
}