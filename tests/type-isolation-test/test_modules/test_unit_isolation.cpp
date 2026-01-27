// Isolate the exact unit type serialization issue
#include "spacetimedb.h"
#include <cstdint>

using namespace SpacetimeDB;

SPACETIMEDB_UNIT_STRUCT(TestUnit)

// Test 1: Just unit + primitive in struct (this should work)
struct UnitPlusInt {
    TestUnit unit;
    int32_t value;
};
SPACETIMEDB_STRUCT(UnitPlusInt, unit, value)
SPACETIMEDB_TABLE(UnitPlusInt, unit_plus_int, Public)

// Test 2: Two units + primitive (this might fail)
SPACETIMEDB_UNIT_STRUCT(SecondUnit)
struct TwoUnits {
    TestUnit unit1;
    SecondUnit unit2;
    int32_t value;
};
SPACETIMEDB_STRUCT(TwoUnits, unit1, unit2, value)
SPACETIMEDB_TABLE(TwoUnits, two_units, Public)

// Test 3: Nested struct with unit (this is where we expect failure)
struct NestedUnitTest {
    UnitPlusInt nested;
    TestUnit another_unit;
};
SPACETIMEDB_STRUCT(NestedUnitTest, nested, another_unit)
SPACETIMEDB_TABLE(NestedUnitTest, nested_unit_test, Public)

// Test different insertion patterns
SPACETIMEDB_REDUCER(test_step1_simple, ReducerContext ctx) {
    TestUnit unit{};
    ctx.db[unit_plus_int].insert({unit, 100});
    return Ok();
}

SPACETIMEDB_REDUCER(test_step2_two_units, ReducerContext ctx) {
    TestUnit unit1{};
    SecondUnit unit2{};
    ctx.db[two_units].insert({unit1, unit2, 200});
    return Ok();
}

SPACETIMEDB_REDUCER(test_step3_nested_fail, ReducerContext ctx) {
    TestUnit unit{};
    UnitPlusInt simple{unit, 300};
    NestedUnitTest nested{simple, unit};
    // This should fail with size mismatch
    ctx.db[nested_unit_test].insert(nested);
    return Ok();
}

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    // Start with simple cases that should work
    TestUnit unit{};
    ctx.db[unit_plus_int].insert({unit, 100});
    
    SecondUnit unit2{};
    ctx.db[two_units].insert({unit, unit2, 200});
    
    // This line should cause the failure:
    UnitPlusInt simple{unit, 300};
    NestedUnitTest nested{simple, unit};
    ctx.db[nested_unit_test].insert(nested);
    return Ok();
}