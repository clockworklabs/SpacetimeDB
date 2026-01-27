#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 4: Large signed integers (I64, I128, I256)
// Testing if large signed integer types cause WASM issues


// OneI64 table
struct OneI64 { int64_t n; };
SPACETIMEDB_STRUCT(OneI64, n)
SPACETIMEDB_TABLE(OneI64, one_i64, Public)

// OneI128 table
struct OneI128 { i128 n; };
SPACETIMEDB_STRUCT(OneI128, n)
SPACETIMEDB_TABLE(OneI128, one_i128, Public)

// OneI256 table
struct OneI256 { i256 n; };
SPACETIMEDB_STRUCT(OneI256, n)
SPACETIMEDB_TABLE(OneI256, one_i256, Public)

// VecI64 table
struct VecI64 { std::vector<int64_t> n; };
SPACETIMEDB_STRUCT(VecI64, n)
SPACETIMEDB_TABLE(VecI64, vec_i64, Public)

// VecI128 table
struct VecI128 { std::vector<i128> n; };
SPACETIMEDB_STRUCT(VecI128, n)
SPACETIMEDB_TABLE(VecI128, vec_i128, Public)

// VecI256 table
struct VecI256 { std::vector<i256> n; };
SPACETIMEDB_STRUCT(VecI256, n)
SPACETIMEDB_TABLE(VecI256, vec_i256, Public)

// UniqueI64 table
struct UniqueI64 { int64_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI64, n, data)
SPACETIMEDB_TABLE(UniqueI64, unique_i64, Public)
FIELD_Unique(unique_i64, n)

// UniqueI128 table
struct UniqueI128 { i128 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI128, n, data)
SPACETIMEDB_TABLE(UniqueI128, unique_i128, Public)
FIELD_Unique(unique_i128, n)

// UniqueI256 table
struct UniqueI256 { i256 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI256, n, data)
SPACETIMEDB_TABLE(UniqueI256, unique_i256, Public)
FIELD_Unique(unique_i256, n)

// PkI64 table
struct PkI64 { int64_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI64, n, data)
SPACETIMEDB_TABLE(PkI64, pk_i64, Public)
FIELD_PrimaryKey(pk_i64, n)

// PkI128 table
struct PkI128 { i128 n; int32_t data; };
SPACETIMEDB_STRUCT(PkI128, n, data)
SPACETIMEDB_TABLE(PkI128, pk_i128, Public)
FIELD_PrimaryKey(pk_i128, n)

// PkI256 table
struct PkI256 { i256 n; int32_t data; };
SPACETIMEDB_STRUCT(PkI256, n, data)
SPACETIMEDB_TABLE(PkI256, pk_i256, Public)
FIELD_PrimaryKey(pk_i256, n)

// Reducers for large signed integers
SPACETIMEDB_REDUCER(insert_one_i64, ReducerContext ctx, int64_t n)
{
    ctx.db.table<OneI64>("one_i64").insert(OneI64{n});
}

SPACETIMEDB_REDUCER(insert_one_i128, ReducerContext ctx, i128 n)
{
    ctx.db.table<OneI128>("one_i128").insert(OneI128{n});
}

SPACETIMEDB_REDUCER(insert_one_i256, ReducerContext ctx, i256 n)
{
    ctx.db.table<OneI256>("one_i256").insert(OneI256{n});
}