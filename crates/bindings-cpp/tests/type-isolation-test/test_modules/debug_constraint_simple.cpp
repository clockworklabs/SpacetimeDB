#include <spacetimedb.h>

using namespace SpacetimeDB;

// Simple test with just one primary key constraint
struct SimpleConstraintTest {
    uint32_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(SimpleConstraintTest, id, data)
SPACETIMEDB_TABLE(SimpleConstraintTest, simple_constraint_test, SpacetimeDB::Public)
FIELD_PrimaryKey(simple_constraint_test, id);

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    LOG_INFO("Simple constraint test initialized");
    return Ok();
}

SPACETIMEDB_REDUCER(test_simple_constraint, SpacetimeDB::ReducerContext ctx) {
    LOG_INFO("Testing simple constraint");
    SimpleConstraintTest test{1, "Test data"};
    ctx.db[simple_constraint_test].insert(test);
    return Ok();
}