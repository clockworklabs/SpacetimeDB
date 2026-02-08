#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 2: Large unsigned integers (U64, U128, U256)
// Testing if large unsigned integer types cause WASM issues


// OneU64 table
struct OneU64 { uint64_t n; };
SPACETIMEDB_STRUCT(OneU64, n)
SPACETIMEDB_TABLE(OneU64, one_u64, Public)

// OneU128 table
struct OneU128 { u128 n; };
SPACETIMEDB_STRUCT(OneU128, n)
SPACETIMEDB_TABLE(OneU128, one_u128, Public)

// OneU256 table
struct OneU256 { u256 n; };
SPACETIMEDB_STRUCT(OneU256, n)
SPACETIMEDB_TABLE(OneU256, one_u256, Public)

// VecU64 table
struct VecU64 { std::vector<uint64_t> n; };
SPACETIMEDB_STRUCT(VecU64, n)
SPACETIMEDB_TABLE(VecU64, vec_u64, Public)

// VecU128 table
struct VecU128 { std::vector<u128> n; };
SPACETIMEDB_STRUCT(VecU128, n)
SPACETIMEDB_TABLE(VecU128, vec_u128, Public)

// VecU256 table
struct VecU256 { std::vector<u256> n; };
SPACETIMEDB_STRUCT(VecU256, n)
SPACETIMEDB_TABLE(VecU256, vec_u256, Public)

// UniqueU64 table
struct UniqueU64 { uint64_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU64, n, data)
SPACETIMEDB_TABLE(UniqueU64, unique_u64, Public)
FIELD_Unique(unique_u64, n)

// UniqueU128 table
struct UniqueU128 { u128 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU128, n, data)
SPACETIMEDB_TABLE(UniqueU128, unique_u128, Public)
FIELD_Unique(unique_u128, n)

// UniqueU256 table
struct UniqueU256 { u256 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU256, n, data)
SPACETIMEDB_TABLE(UniqueU256, unique_u256, Public)
FIELD_Unique(unique_u256, n)

// PkU64 table
struct PkU64 { uint64_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU64, n, data)
SPACETIMEDB_TABLE(PkU64, pk_u64, Public)
FIELD_PrimaryKey(pk_u64, n)

// PkU128 table
struct PkU128 { u128 n; int32_t data; };
SPACETIMEDB_STRUCT(PkU128, n, data)
SPACETIMEDB_TABLE(PkU128, pk_u128, Public)
FIELD_PrimaryKey(pk_u128, n)

// PkU256 table
struct PkU256 { u256 n; int32_t data; };
SPACETIMEDB_STRUCT(PkU256, n, data)
SPACETIMEDB_TABLE(PkU256, pk_u256, Public)
FIELD_PrimaryKey(pk_u256, n)

// Reducers for large unsigned integers
SPACETIMEDB_REDUCER(insert_one_u64, ReducerContext ctx, uint64_t n)
{
    ctx.db.table<OneU64>("one_u64").insert(OneU64{n});
}

SPACETIMEDB_REDUCER(insert_one_u128, ReducerContext ctx, u128 n)
{
    ctx.db.table<OneU128>("one_u128").insert(OneU128{n});
}

SPACETIMEDB_REDUCER(insert_one_u256, ReducerContext ctx, u256 n)
{
    ctx.db.table<OneU256>("one_u256").insert(OneU256{n});
}