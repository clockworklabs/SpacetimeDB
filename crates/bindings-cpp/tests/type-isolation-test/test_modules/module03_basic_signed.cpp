#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 3: Basic signed integers (I8, I16, I32)
// Testing if basic signed integer types cause WASM issues


// OneI8 table
struct OneI8 { int8_t n; };
SPACETIMEDB_STRUCT(OneI8, n)
SPACETIMEDB_TABLE(OneI8, one_i8, Public)

// OneI16 table
struct OneI16 { int16_t n; };
SPACETIMEDB_STRUCT(OneI16, n)
SPACETIMEDB_TABLE(OneI16, one_i16, Public)

// OneI32 table
struct OneI32 { int32_t n; };
SPACETIMEDB_STRUCT(OneI32, n)
SPACETIMEDB_TABLE(OneI32, one_i32, Public)

// VecI8 table
struct VecI8 { std::vector<int8_t> n; };
SPACETIMEDB_STRUCT(VecI8, n)
SPACETIMEDB_TABLE(VecI8, vec_i8, Public)

// VecI16 table
struct VecI16 { std::vector<int16_t> n; };
SPACETIMEDB_STRUCT(VecI16, n)
SPACETIMEDB_TABLE(VecI16, vec_i16, Public)

// VecI32 table
struct VecI32 { std::vector<int32_t> n; };
SPACETIMEDB_STRUCT(VecI32, n)
SPACETIMEDB_TABLE(VecI32, vec_i32, Public)

// UniqueI8 table
struct UniqueI8 { int8_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI8, n, data)
SPACETIMEDB_TABLE(UniqueI8, unique_i8, Public)
FIELD_Unique(unique_i8, n)

// UniqueI16 table
struct UniqueI16 { int16_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI16, n, data)
SPACETIMEDB_TABLE(UniqueI16, unique_i16, Public)
FIELD_Unique(unique_i16, n)

// UniqueI32 table
struct UniqueI32 { int32_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI32, n, data)
SPACETIMEDB_TABLE(UniqueI32, unique_i32, Public)
FIELD_Unique(unique_i32, n)

// PkI8 table
struct PkI8 { int8_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI8, n, data)
SPACETIMEDB_TABLE(PkI8, pk_i8, Public)
FIELD_PrimaryKey(pk_i8, n)

// PkI16 table
struct PkI16 { int16_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI16, n, data)
SPACETIMEDB_TABLE(PkI16, pk_i16, Public)
FIELD_PrimaryKey(pk_i16, n)

// PkI32 table
struct PkI32 { int32_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI32, n, data)
SPACETIMEDB_TABLE(PkI32, pk_i32, Public)
FIELD_PrimaryKey(pk_i32, n)

// Reducers for basic signed integers
SPACETIMEDB_REDUCER(insert_one_i8, ReducerContext ctx, int8_t n)
{
    ctx.db.table<OneI8>("one_i8").insert(OneI8{n});
}

SPACETIMEDB_REDUCER(insert_one_i16, ReducerContext ctx, int16_t n)
{
    ctx.db.table<OneI16>("one_i16").insert(OneI16{n});
}

SPACETIMEDB_REDUCER(insert_one_i32, ReducerContext ctx, int32_t n)
{
    ctx.db.table<OneI32>("one_i32").insert(OneI32{n});
}