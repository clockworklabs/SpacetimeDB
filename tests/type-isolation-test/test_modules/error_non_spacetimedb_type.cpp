#include <spacetimedb.h>
#include <memory>
#include <thread>
#include <atomic>

// Test types that don't have BSATN serialization support
// These should be caught as compilation errors

// Custom type without SPACETIMEDB_STRUCT macro
struct UnsupportedType {
    int x;
    int y;
};
// Intentionally NOT adding: SPACETIMEDB_STRUCT(UnsupportedType, x, y)

// Type with unsupported member (std::thread)
struct ThreadContainingType {
    uint32_t id;
    std::thread worker;  // std::thread cannot be serialized
};
// This would fail even with SPACETIMEDB_STRUCT

// Type with raw pointer (not serializable)
struct RawPointerType {
    uint32_t id;
    int* data;  // Raw pointers cannot be serialized
};
// SPACETIMEDB_STRUCT(RawPointerType, id, data) // Would fail

// Type with std::unique_ptr (not serializable)
struct SmartPointerType {
    uint32_t id;
    std::unique_ptr<int> value;  // Smart pointers cannot be serialized
};
// SPACETIMEDB_STRUCT(SmartPointerType, id, value) // Would fail

// Type with std::atomic (not serializable)
struct AtomicType {
    uint32_t id;
    std::atomic<int> counter;  // Atomics cannot be serialized
};
// SPACETIMEDB_STRUCT(AtomicType, id, counter) // Would fail

// Valid type for comparison
struct ValidType {
    uint32_t id;
    std::string name;
};
SPACETIMEDB_STRUCT(ValidType, id, name)

// Try to use unsupported type as table - should fail
SPACETIMEDB_TABLE(UnsupportedType, unsupported_table, SpacetimeDB::Public)

// Try to use unsupported type as reducer argument - should fail
SPACETIMEDB_REDUCER(test_unsupported_arg, SpacetimeDB::ReducerContext ctx, UnsupportedType arg)
{
    LOG_INFO("This should never compile - UnsupportedType lacks BSATN traits");
    return Ok();
}

// Valid reducer for comparison
SPACETIMEDB_REDUCER(test_valid_arg, SpacetimeDB::ReducerContext ctx, ValidType arg)
{
    LOG_INFO("Valid type works fine: " + arg.name);
    return Ok();
}

// Try to use type with unsupported members in a struct
struct ComplexBadType {
    uint32_t id;
    UnsupportedType unsupported;  // Contains type without BSATN
};
SPACETIMEDB_STRUCT(ComplexBadType, id, unsupported)  // Should fail - nested type lacks traits
SPACETIMEDB_TABLE(ComplexBadType, complex_bad_table, SpacetimeDB::Public)

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx)
{
    LOG_INFO("Non-SpacetimeDB type test - should fail compilation");
    return Ok();
}