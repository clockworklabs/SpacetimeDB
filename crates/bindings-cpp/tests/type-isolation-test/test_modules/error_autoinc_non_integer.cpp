#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test auto-increment on non-integer column types
// AutoInc should only work on integer types (uint32_t, uint64_t, etc.)

// AutoInc on string field - INVALID
struct StringAutoInc {
    std::string id;  // Strings can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(StringAutoInc, id, data)
SPACETIMEDB_TABLE(StringAutoInc, string_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(string_autoinc_table, id);  // ERROR: AutoInc on string!

// AutoInc on float field - INVALID
struct FloatAutoInc {
    float id;  // Floats can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(FloatAutoInc, id, data)
SPACETIMEDB_TABLE(FloatAutoInc, float_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(float_autoinc_table, id);  // ERROR: AutoInc on float!

// AutoInc on double field - INVALID
struct DoubleAutoInc {
    double id;  // Doubles can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(DoubleAutoInc, id, data)
SPACETIMEDB_TABLE(DoubleAutoInc, double_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(double_autoinc_table, id);  // ERROR: AutoInc on double!

// AutoInc on bool field - INVALID
struct BoolAutoInc {
    bool id;  // Bools can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(BoolAutoInc, id, data)
SPACETIMEDB_TABLE(BoolAutoInc, bool_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(bool_autoinc_table, id);  // ERROR: AutoInc on bool!

// AutoInc on Identity field - INVALID (Identity is not an integer)
struct IdentityAutoInc {
    Identity id;  // Identity can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(IdentityAutoInc, id, data)
SPACETIMEDB_TABLE(IdentityAutoInc, identity_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(identity_autoinc_table, id);  // ERROR: AutoInc on Identity!

// AutoInc on struct field - INVALID
struct NestedStruct {
    uint32_t x;
    uint32_t y;
};
SPACETIMEDB_STRUCT(NestedStruct, x, y)

struct StructAutoInc {
    NestedStruct id;  // Structs can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(StructAutoInc, id, data)
SPACETIMEDB_TABLE(StructAutoInc, struct_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(struct_autoinc_table, id);  // ERROR: AutoInc on struct!

// AutoInc on vector field - INVALID
struct VectorAutoInc {
    std::vector<uint32_t> id;  // Vectors can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(VectorAutoInc, id, data)
SPACETIMEDB_TABLE(VectorAutoInc, vector_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(vector_autoinc_table, id);  // ERROR: AutoInc on vector!

// AutoInc on optional field - INVALID
struct OptionalAutoInc {
    std::optional<uint32_t> id;  // Optionals can't auto-increment!
    std::string data;
};
SPACETIMEDB_STRUCT(OptionalAutoInc, id, data)
SPACETIMEDB_TABLE(OptionalAutoInc, optional_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(optional_autoinc_table, id);  // ERROR: AutoInc on optional!

// Valid AutoInc tables for comparison
struct ValidU32AutoInc {
    uint32_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidU32AutoInc, id, data)
SPACETIMEDB_TABLE(ValidU32AutoInc, valid_u32_autoinc, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(valid_u32_autoinc, id);  // Correct: AutoInc on uint32_t

struct ValidU64AutoInc {
    uint64_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidU64AutoInc, id, data)
SPACETIMEDB_TABLE(ValidU64AutoInc, valid_u64_autoinc, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(valid_u64_autoinc, id);  // Correct: AutoInc on uint64_t

struct ValidI32AutoInc {
    int32_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidI32AutoInc, id, data)
SPACETIMEDB_TABLE(ValidI32AutoInc, valid_i32_autoinc, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(valid_i32_autoinc, id);  // Correct: AutoInc on int32_t

struct ValidI64AutoInc {
    int64_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidI64AutoInc, id, data)
SPACETIMEDB_TABLE(ValidI64AutoInc, valid_i64_autoinc, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(valid_i64_autoinc, id);  // Correct: AutoInc on int64_t

// Test reducer
SPACETIMEDB_REDUCER(test_autoinc_types, SpacetimeDB::ReducerContext ctx)
{
    LOG_INFO("Testing auto-increment on non-integer types - should fail validation");
    
    // These should all fail if validation works
    StringAutoInc string_ai{"", "String AutoInc"};
    ctx.db[string_autoinc_table].insert(string_ai);
    
    FloatAutoInc float_ai{0.0f, "Float AutoInc"};
    ctx.db[float_autoinc_table].insert(float_ai);
    
    // Valid inserts
    ValidU32AutoInc valid_u32{0, "Valid U32"};
    ctx.db[valid_u32_autoinc].insert(valid_u32);
    
    ValidU64AutoInc valid_u64{0, "Valid U64"};
    ctx.db[valid_u64_autoinc].insert(valid_u64);
    return Ok();
}

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx)
{
    LOG_INFO("Auto-increment on non-integer types test");
    LOG_INFO("AutoInc should only work on integer types");
    return Ok();
}