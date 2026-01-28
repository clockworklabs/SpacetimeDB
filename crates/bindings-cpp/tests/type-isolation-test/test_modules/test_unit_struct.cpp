// Test module for unit struct types
#include "spacetimedb.h"
#include <cstdint>

using namespace SpacetimeDB;

// Define various unit structs using the macro
SPACETIMEDB_UNIT_STRUCT(BasicUnit)
SPACETIMEDB_UNIT_STRUCT(AnotherUnit)
SPACETIMEDB_UNIT_STRUCT(ThirdUnit)

// Removed monostate struct - causes issues

// Regular struct that contains unit types
struct StructWithUnits {
    int32_t id;
    BasicUnit basic;
    AnotherUnit another;
    std::string name;
};
SPACETIMEDB_STRUCT(StructWithUnits, id, basic, another, name)

// Nested struct with units
struct NestedWithUnits {
    StructWithUnits nested;
    ThirdUnit third;
    int32_t value;
};
SPACETIMEDB_STRUCT(NestedWithUnits, nested, third, value)

// Table with unit fields
struct TableWithUnits {
    uint32_t id_field;
    BasicUnit unit_field;
    int32_t data;
};
SPACETIMEDB_STRUCT(TableWithUnits, id_field, unit_field, data)
SPACETIMEDB_TABLE(TableWithUnits, table_with_units, Public)

// Another table with nested units
struct ComplexTable {
    uint32_t key_field;
    StructWithUnits complex_field;
    std::string description;
};
SPACETIMEDB_STRUCT(ComplexTable, key_field, complex_field, description)
SPACETIMEDB_TABLE(ComplexTable, complex_table, Public)

// Table that is just units and primitives
struct SimpleUnitTable {
    BasicUnit unit1;
    AnotherUnit unit2;
    int32_t value;
};
SPACETIMEDB_STRUCT(SimpleUnitTable, unit1, unit2, value)
SPACETIMEDB_TABLE(SimpleUnitTable, simple_unit_table, Public)

// Reducers that use unit types

// Reducer with unit parameter
SPACETIMEDB_REDUCER(reducer_with_unit_param, ReducerContext ctx, BasicUnit unit_param, int32_t value) {
    // Insert into table
    ctx.db[table_with_units].insert({
        static_cast<uint32_t>(value),
        unit_param,
        value * 2
    });
    return Ok();
}

// Reducer with struct containing units
SPACETIMEDB_REDUCER(reducer_with_struct_param, ReducerContext ctx, StructWithUnits struct_param) {
    // Insert into complex table
    ctx.db[complex_table].insert({
        static_cast<uint32_t>(struct_param.id),
        struct_param,
        "From reducer"
    });
    return Ok();
}

// Reducer with multiple unit parameters
SPACETIMEDB_REDUCER(reducer_multiple_units, ReducerContext ctx, 
                    BasicUnit unit1, AnotherUnit unit2, ThirdUnit unit3, int32_t id) {
    // Insert simple unit table
    ctx.db[simple_unit_table].insert({
        unit1,
        unit2,
        id
    });
    return Ok();
}

// Reducer returning unit type in struct
SPACETIMEDB_REDUCER(reducer_nested_units, ReducerContext ctx, NestedWithUnits nested) {
    // Access nested units
    ctx.db[complex_table].insert({
        static_cast<uint32_t>(nested.value),
        nested.nested,
        "Nested units"
    });
    return Ok();
}

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    // Create some initial data with units
    BasicUnit basic{};
    AnotherUnit another{};
    
    ctx.db[table_with_units].insert({1, basic, 100});
    
    StructWithUnits s{42, basic, another, "initial"};
    ctx.db[complex_table].insert({1, s, "Initial entry"});
    
    ctx.db[simple_unit_table].insert({basic, another, 999});
    return Ok();
}