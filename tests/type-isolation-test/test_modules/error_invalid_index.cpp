#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test FIELD_ macro validation for index constraints on non-filterable types

// Complex struct that can't be indexed
struct ComplexData {
    uint32_t x;
    uint32_t y;
    std::string label;
};
SPACETIMEDB_STRUCT(ComplexData, x, y, label)

// Table with struct field - define without constraints in SPACETIMEDB_TABLE
struct UniqueOnStruct {
    uint32_t id;
    ComplexData data;
    std::string name;
};
SPACETIMEDB_STRUCT(UniqueOnStruct, id, data, name)

// Register table without any constraints
SPACETIMEDB_TABLE(UniqueOnStruct, unique_struct_table, SpacetimeDB::Public)

// Now add constraints using FIELD_ macros - this should fail at compile time!
FIELD_PrimaryKey(unique_struct_table, id)           // OK: Any type can be primary key
FIELD_Unique(unique_struct_table, data)             // ERROR: ComplexData is not filterable!
FIELD_Index(unique_struct_table, name)              // OK: String is filterable

// Table with vector field
struct UniqueOnVector {
    uint32_t id;
    std::vector<uint32_t> items;
    std::string name;
};
SPACETIMEDB_STRUCT(UniqueOnVector, id, items, name)
SPACETIMEDB_TABLE(UniqueOnVector, unique_vector_table, SpacetimeDB::Public)

// Add field constraints
FIELD_PrimaryKey(unique_vector_table, id)
FIELD_Unique(unique_vector_table, items)            // ERROR: Vector is not filterable!

// Table with optional field
struct UniqueOnOptional {
    uint32_t id;
    std::optional<uint32_t> maybe_value;
    std::string name;
};
SPACETIMEDB_STRUCT(UniqueOnOptional, id, maybe_value, name)
SPACETIMEDB_TABLE(UniqueOnOptional, unique_optional_table, SpacetimeDB::Public)

// Add field constraints
FIELD_PrimaryKey(unique_optional_table, id)
FIELD_Unique(unique_optional_table, maybe_value)    // ERROR: Optional is not filterable!

// Table with float field
struct UniqueOnFloat {
    uint32_t id;
    float value;
    std::string name;
};
SPACETIMEDB_STRUCT(UniqueOnFloat, id, value, name)
SPACETIMEDB_TABLE(UniqueOnFloat, unique_float_table, SpacetimeDB::Public)

// Add field constraints
FIELD_PrimaryKey(unique_float_table, id)
FIELD_Unique(unique_float_table, value)             // ERROR: Float is not filterable!

// Table with double field
struct IndexOnDouble {
    uint32_t id;
    double value;
    std::string name;
};
SPACETIMEDB_STRUCT(IndexOnDouble, id, value, name)
SPACETIMEDB_TABLE(IndexOnDouble, index_double_table, SpacetimeDB::Public)

// Add field constraints
FIELD_PrimaryKey(index_double_table, id)
FIELD_Index(index_double_table, value)              // ERROR: Double is not filterable!

// Table with ScheduleAt field
struct UniqueOnScheduleAt {
    uint32_t id;
    ScheduleAt schedule;
    std::string name;
};
SPACETIMEDB_STRUCT(UniqueOnScheduleAt, id, schedule, name)
SPACETIMEDB_TABLE(UniqueOnScheduleAt, unique_schedule_table, SpacetimeDB::Public)

// Add field constraints
FIELD_PrimaryKey(unique_schedule_table, id)
FIELD_Unique(unique_schedule_table, schedule)       // ERROR: ScheduleAt is not filterable!

// Valid indexed tables for comparison
struct ValidUniqueInt {
    uint32_t id;
    uint32_t unique_code;
    std::string name;
};
SPACETIMEDB_STRUCT(ValidUniqueInt, id, unique_code, name)
SPACETIMEDB_TABLE(ValidUniqueInt, valid_unique_int_table, SpacetimeDB::Public)

// These should all work fine
FIELD_PrimaryKey(valid_unique_int_table, id)
FIELD_Unique(valid_unique_int_table, unique_code)   // OK: Integer is filterable

struct ValidIndexString {
    uint32_t id;
    std::string indexed_name;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidIndexString, id, indexed_name, data)
SPACETIMEDB_TABLE(ValidIndexString, valid_index_string_table, SpacetimeDB::Public)

FIELD_PrimaryKey(valid_index_string_table, id)
FIELD_Index(valid_index_string_table, indexed_name) // OK: String is filterable

struct ValidUniqueIdentity {
    uint32_t id;
    Identity user_id;
    std::string name;
};
SPACETIMEDB_STRUCT(ValidUniqueIdentity, id, user_id, name)
SPACETIMEDB_TABLE(ValidUniqueIdentity, valid_unique_identity_table, SpacetimeDB::Public)

FIELD_PrimaryKey(valid_unique_identity_table, id)
FIELD_Unique(valid_unique_identity_table, user_id)  // OK: Identity is filterable

struct ValidIndexTimestamp {
    uint32_t id;
    Timestamp created_at;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidIndexTimestamp, id, created_at, data)
SPACETIMEDB_TABLE(ValidIndexTimestamp, valid_index_timestamp_table, SpacetimeDB::Public)

FIELD_PrimaryKey(valid_index_timestamp_table, id)
FIELD_Index(valid_index_timestamp_table, created_at) // OK: Timestamp is filterable

struct ValidUniqueBool {
    uint32_t id;
    bool is_active;
    std::string data;
};
SPACETIMEDB_STRUCT(ValidUniqueBool, id, is_active, data)
SPACETIMEDB_TABLE(ValidUniqueBool, valid_unique_bool_table, SpacetimeDB::Public)

FIELD_PrimaryKey(valid_unique_bool_table, id)
FIELD_Unique(valid_unique_bool_table, is_active)    // OK: Bool is filterable (though unusual)

// Test reducer
SPACETIMEDB_REDUCER(test_field_macro_validation, SpacetimeDB::ReducerContext ctx)
{
    LOG_INFO("Testing FIELD_ macro validation");
    
    // If this code runs, it means validation failed!
    // The module should fail to compile due to FIELD_ macro validation
    
    ComplexData complex{1, 2, "Complex"};
    UniqueOnStruct bad_unique{1, complex, "Bad unique"};
    ctx.db[unique_struct_table].insert(bad_unique);
    
    ValidUniqueInt valid_int{1, 100, "Valid unique int"};
    ctx.db[valid_unique_int_table].insert(valid_int);
    
    ValidIndexString valid_string{1, "indexed", "Valid index string"};
    ctx.db[valid_index_string_table].insert(valid_string);
    return Ok();
}

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx)
{
    LOG_INFO("FIELD_ macro validation test");
    LOG_INFO("This module should FAIL to compile if validation is working");
    LOG_INFO("Errors expected for:");
    LOG_INFO("- FIELD_Unique on ComplexData");
    LOG_INFO("- FIELD_Unique on vector");
    LOG_INFO("- FIELD_Unique on optional");
    LOG_INFO("- FIELD_Unique on float");
    LOG_INFO("- FIELD_Index on double");
    LOG_INFO("- FIELD_Unique on ScheduleAt");
    return Ok();
}