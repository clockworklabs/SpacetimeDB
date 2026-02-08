#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 1: Basic unsigned integers (U8, U16, U32)
// Testing if basic unsigned integer types cause WASM issues


// OneU8 table
struct OneU8 { uint8_t n; };
SPACETIMEDB_STRUCT(OneU8, n)
SPACETIMEDB_TABLE(OneU8, one_u8, Public)

// OneU16 table  
struct OneU16 { uint16_t n; };
SPACETIMEDB_STRUCT(OneU16, n)
SPACETIMEDB_TABLE(OneU16, one_u16, Public)

// OneU32 table
struct OneU32 { uint32_t n; };
SPACETIMEDB_STRUCT(OneU32, n)
SPACETIMEDB_TABLE(OneU32, one_u32, Public)

// VecU8 table
struct VecU8 { std::vector<uint8_t> n; };
SPACETIMEDB_STRUCT(VecU8, n)
SPACETIMEDB_TABLE(VecU8, vec_u8, Public)

// VecU16 table
struct VecU16 { std::vector<uint16_t> n; };
SPACETIMEDB_STRUCT(VecU16, n)
SPACETIMEDB_TABLE(VecU16, vec_u16, Public)

// VecU32 table
struct VecU32 { std::vector<uint32_t> n; };
SPACETIMEDB_STRUCT(VecU32, n)
SPACETIMEDB_TABLE(VecU32, vec_u32, Public)

// UniqueU8 table
struct UniqueU8 { uint8_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU8, n, data)
SPACETIMEDB_TABLE(UniqueU8, unique_u8, Public)
FIELD_Unique(unique_u8, n)

// UniqueU16 table
struct UniqueU16 { uint16_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU16, n, data)
SPACETIMEDB_TABLE(UniqueU16, unique_u16, Public)
FIELD_Unique(unique_u16, n)

// UniqueU32 table
struct UniqueU32 { uint32_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU32, n, data)
SPACETIMEDB_TABLE(UniqueU32, unique_u32, Public)
FIELD_Unique(unique_u32, n)

// PkU8 table
struct PkU8 { uint8_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU8, n, data)
SPACETIMEDB_TABLE(PkU8, pk_u8, Public)
FIELD_PrimaryKey(pk_u8, n)

// PkU16 table
struct PkU16 { uint16_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU16, n, data)
SPACETIMEDB_TABLE(PkU16, pk_u16, Public)
FIELD_PrimaryKey(pk_u16, n)

// PkU32 table
struct PkU32 { uint32_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU32, n, data)
SPACETIMEDB_TABLE(PkU32, pk_u32, Public)
FIELD_PrimaryKey(pk_u32, n)

// Reducers for basic unsigned integers
SPACETIMEDB_REDUCER(insert_one_u8, ReducerContext ctx, uint8_t n)
{
    ctx.db.table<OneU8>("one_u8").insert(OneU8{n});
}

SPACETIMEDB_REDUCER(insert_one_u16, ReducerContext ctx, uint16_t n)
{
    ctx.db.table<OneU16>("one_u16").insert(OneU16{n});
}

SPACETIMEDB_REDUCER(insert_one_u32, ReducerContext ctx, uint32_t n)
{
    ctx.db.table<OneU32>("one_u32").insert(OneU32{n});
}