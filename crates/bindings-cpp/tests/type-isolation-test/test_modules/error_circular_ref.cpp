#include <spacetimedb.h>
#include <vector>
#include <optional>
// Test circular references: A → B → C → A
// This should be caught by the type system




// StructC references StructA (completing the circle)
struct StructA {
    uint32_t id;
    std::vector<StructA> a_ref; //(circular!)
};

// Register the structs
SPACETIMEDB_STRUCT(StructA, id, a_ref)

// Try to use them as tables
SPACETIMEDB_TABLE(StructA, struct_a, SpacetimeDB::Public)

// Test reducer
SPACETIMEDB_REDUCER(test_circular_ref, SpacetimeDB::ReducerContext ctx)
{
    LOG_INFO("This should never execute - circular references should be detected");
}