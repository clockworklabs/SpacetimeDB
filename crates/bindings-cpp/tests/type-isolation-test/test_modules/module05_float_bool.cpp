#include <spacetimedb.h>
// Removed: enhanced_database.h (unused functionality)

using namespace SpacetimeDB;

// Module 5: Floating point and boolean types
// Testing if float/bool types cause WASM issues


// OneBool table
struct OneBool { bool b; };
SPACETIMEDB_STRUCT(OneBool, b)
SPACETIMEDB_TABLE(OneBool, one_bool, Public)

// OneF32 table
struct OneF32 { float f; };
SPACETIMEDB_STRUCT(OneF32, f)
SPACETIMEDB_TABLE(OneF32, one_f32, Public)

// OneF64 table
struct OneF64 { double f; };
SPACETIMEDB_STRUCT(OneF64, f)
SPACETIMEDB_TABLE(OneF64, one_f64, Public)

// VecBool table
struct VecBool { std::vector<bool> b; };
SPACETIMEDB_STRUCT(VecBool, b)
SPACETIMEDB_TABLE(VecBool, vec_bool, Public)

// VecF32 table
struct VecF32 { std::vector<float> f; };
SPACETIMEDB_STRUCT(VecF32, f)
SPACETIMEDB_TABLE(VecF32, vec_f32, Public)

// VecF64 table
struct VecF64 { std::vector<double> f; };
SPACETIMEDB_STRUCT(VecF64, f)
SPACETIMEDB_TABLE(VecF64, vec_f64, Public)

// UniqueBool table
struct UniqueBool { bool b; int32_t data; };
SPACETIMEDB_STRUCT(UniqueBool, b, data)
SPACETIMEDB_TABLE(UniqueBool, unique_bool, Public)
FIELD_Unique(unique_bool, b)

// PkBool table
struct PkBool { bool b; int32_t data; };
SPACETIMEDB_STRUCT(PkBool, b, data)
SPACETIMEDB_TABLE(PkBool, pk_bool, Public)
FIELD_PrimaryKey(pk_bool, b)

// Reducers for float and bool types
SPACETIMEDB_REDUCER(insert_one_bool, ReducerContext ctx, bool b)
{
    ctx.db.table<OneBool>("one_bool").insert(OneBool{b});
}

SPACETIMEDB_REDUCER(insert_one_f32, ReducerContext ctx, float f)
{
    ctx.db.table<OneF32>("one_f32").insert(OneF32{f});
}

SPACETIMEDB_REDUCER(insert_one_f64, ReducerContext ctx, double f)
{
    ctx.db.table<OneF64>("one_f64").insert(OneF64{f});
}