#include <spacetimedb.h>

using namespace SpacetimeDB;

// Module 6: String and text types
// Testing if string types cause WASM issues


// OneString table
struct OneString { std::string s; };
SPACETIMEDB_STRUCT(OneString, s)
SPACETIMEDB_TABLE(OneString, one_string, Public)

// VecString table
struct VecString { std::vector<std::string> s; };
SPACETIMEDB_STRUCT(VecString, s)
SPACETIMEDB_TABLE(VecString, vec_string, Public)

// UniqueString table
struct UniqueString { std::string s; int32_t data; };
SPACETIMEDB_STRUCT(UniqueString, s, data)
SPACETIMEDB_TABLE(UniqueString, unique_string, Public)
FIELD_Unique(unique_string, s)

// PkString table
struct PkString { std::string s; int32_t data; };
SPACETIMEDB_STRUCT(PkString, s, data)
SPACETIMEDB_TABLE(PkString, pk_string, Public)
FIELD_PrimaryKey(pk_string, s)

// Reducer for string types
SPACETIMEDB_REDUCER(insert_one_string, ReducerContext ctx, std::string s)
{
    ctx.db.table<OneString>("one_string").insert(OneString{s});
}