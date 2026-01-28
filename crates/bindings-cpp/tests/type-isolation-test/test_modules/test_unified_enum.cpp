#include <spacetimedb.h>

using namespace SpacetimeDB;


// Test 1: Simple enum (should route to SPACETIMEDB_ENUM_SIMPLE)
SPACETIMEDB_ENUM(SimpleTestEnum, Red, Green, Blue)

// Test 2: Complex enum using unified syntax
SPACETIMEDB_ENUM(ComplexTestEnum, (Number, uint32_t), (Text, std::string))

// Test table with simple enum
struct TestTable {
    uint32_t id;
    SimpleTestEnum color;
};
SPACETIMEDB_STRUCT(TestTable, id, color)
SPACETIMEDB_TABLE(TestTable, test_table, Public)

// Test reducer with complex enum parameter
SPACETIMEDB_REDUCER(test_complex_enum, ReducerContext ctx, ComplexTestEnum value) {
    LOG_INFO("Complex enum test completed");
}