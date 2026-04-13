#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test enum variants containing vectors - the missing pattern from lib.cpp
SPACETIMEDB_ENUM(SimpleEnum, A, B, C)

// Critical test: Enum with vector payloads (especially vector of enums)
SPACETIMEDB_ENUM(EnumWithVectorPayloads,
    (Bytes, std::vector<uint8_t>),
    (Ints, std::vector<int32_t>),
    (Strings, std::vector<std::string>),
    (SimpleEnums, std::vector<SimpleEnum>)  // Vector of enums in enum variant!
)

// Table using the complex enum
struct TableWithComplexEnum {
    EnumWithVectorPayloads complex_enum;
};
SPACETIMEDB_STRUCT(TableWithComplexEnum, complex_enum)
SPACETIMEDB_TABLE(TableWithComplexEnum, table_with_complex_enum, Public)

// Even more complex: Vector of enums that contain vectors
struct TableWithVectorOfComplexEnums {
    std::vector<EnumWithVectorPayloads> vec_of_complex_enums;
};
SPACETIMEDB_STRUCT(TableWithVectorOfComplexEnums, vec_of_complex_enums)
SPACETIMEDB_TABLE(TableWithVectorOfComplexEnums, table_with_vector_of_complex_enums, Public)

// Test reducers
SPACETIMEDB_REDUCER(test_complex_enum, ReducerContext ctx, EnumWithVectorPayloads e)
{
    ctx.db[table_with_complex_enum].insert(TableWithComplexEnum{e});
}

SPACETIMEDB_REDUCER(test_vector_complex_enum, ReducerContext ctx, std::vector<EnumWithVectorPayloads> vec)
{
    ctx.db[table_with_vector_of_complex_enums].insert(TableWithVectorOfComplexEnums{vec});
}