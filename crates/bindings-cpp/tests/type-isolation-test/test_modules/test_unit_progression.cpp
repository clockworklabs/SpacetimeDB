// Progressive unit type testing to isolate the exact failure
#include "spacetimedb.h"
#include <cstdint>

using namespace SpacetimeDB;

// Test 1: Single unit (we know this works)
SPACETIMEDB_UNIT_STRUCT(TestUnit)

// Test 2: Multiple units
SPACETIMEDB_UNIT_STRUCT(Unit1)
SPACETIMEDB_UNIT_STRUCT(Unit2)

// Test 3: Unit in simple struct
struct SimpleStructWithUnit {
    TestUnit unit;
    int32_t data;
};
SPACETIMEDB_STRUCT(SimpleStructWithUnit, unit, data)

// Test 4: Multiple units in struct
struct StructWithMultipleUnits {
    Unit1 unit1;
    Unit2 unit2;
    int32_t value;
};
SPACETIMEDB_STRUCT(StructWithMultipleUnits, unit1, unit2, value)

// Test 5: Table with unit field
struct TableWithUnit {
    TestUnit unit;
    int32_t id;
};
SPACETIMEDB_STRUCT(TableWithUnit, unit, id)
SPACETIMEDB_TABLE(TableWithUnit, table_with_unit, Public)

// Test 6: Table with multiple units  
struct TableWithMultipleUnits {
    Unit1 unit1;
    Unit2 unit2;
    int32_t id;
};
SPACETIMEDB_STRUCT(TableWithMultipleUnits, unit1, unit2, id)
SPACETIMEDB_TABLE(TableWithMultipleUnits, table_with_multiple_units, Public)

// Test 7: Nested struct with units
struct NestedUnit {
    SimpleStructWithUnit nested;
    TestUnit unit;
};
SPACETIMEDB_STRUCT(NestedUnit, nested, unit)
SPACETIMEDB_TABLE(NestedUnit, nested_unit_table, Public)

// Reducers to test each step
SPACETIMEDB_REDUCER(test_single_unit, ReducerContext ctx, TestUnit unit) {
    ctx.db[table_with_unit].insert({unit, 1});
    return Ok();
}

SPACETIMEDB_REDUCER(test_multiple_units, ReducerContext ctx, Unit1 u1, Unit2 u2) {
    ctx.db[table_with_multiple_units].insert({u1, u2, 2});
    return Ok();
}

SPACETIMEDB_REDUCER(test_struct_with_unit, ReducerContext ctx, SimpleStructWithUnit s) {
    ctx.db[table_with_unit].insert({s.unit, s.data});
    return Ok();
}

SPACETIMEDB_REDUCER(test_nested_units, ReducerContext ctx, NestedUnit nested) {
    ctx.db[nested_unit_table].insert(nested);
    return Ok();
}

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    TestUnit test_unit{};
    Unit1 unit1{};
    Unit2 unit2{};
    
    // Test basic operations
    ctx.db[table_with_unit].insert({test_unit, 100});
    ctx.db[table_with_multiple_units].insert({unit1, unit2, 200});
    
    SimpleStructWithUnit simple{test_unit, 300};
    NestedUnit nested{simple, test_unit};
    ctx.db[nested_unit_table].insert(nested);
    return Ok();
}