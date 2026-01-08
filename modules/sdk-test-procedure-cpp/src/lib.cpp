#include "spacetimedb.h"
#include "spacetimedb/procedure_macros.h"

using namespace SpacetimeDb;

// ============================================================================
// Test Types
// ============================================================================

struct ReturnStruct {
    uint32_t a;
    std::string b;
};
SPACETIMEDB_STRUCT(ReturnStruct, a, b)

SPACETIMEDB_ENUM(ReturnEnum,
    (A, uint32_t),
    (B, std::string)
)

// Table for transaction tests
struct MyTable {
    ReturnStruct field;
};
SPACETIMEDB_STRUCT(MyTable, field)
SPACETIMEDB_TABLE(MyTable, my_table, Public)

// ============================================================================
// Procedure Tests - Part 1: Return Values
// ============================================================================

// Test returning a primitive type
SPACETIMEDB_PROCEDURE(uint32_t, return_primitive, ProcedureContext ctx, uint32_t lhs, uint32_t rhs) {
    return lhs + rhs;
}

// Test returning a struct
SPACETIMEDB_PROCEDURE(ReturnStruct, return_struct, ProcedureContext ctx, uint32_t a, std::string b) {
    return ReturnStruct{a, b};
}

// Test returning enum variant A
SPACETIMEDB_PROCEDURE(ReturnEnum, return_enum_a, ProcedureContext ctx, uint32_t a) {
    return ReturnEnum{a};
}

// Test returning enum variant B
SPACETIMEDB_PROCEDURE(ReturnEnum, return_enum_b, ProcedureContext ctx, std::string b) {
    return ReturnEnum{b};
}

// Test procedure that panics
SPACETIMEDB_PROCEDURE(Unit, will_panic, ProcedureContext ctx) {
    LOG_PANIC("This procedure is expected to panic");
    return Unit{};  // Never reached
}

// ============================================================================
// Procedure Tests - Part 2: Transactions
// ============================================================================
#ifdef SPACETIMEDB_UNSTABLE_FEATURES

// Helper function to insert a row
void insert_my_table(TxContext& tx) {
    tx.db[my_table].insert(MyTable{
        ReturnStruct{42, "magic"}
    });
}

// Helper function to assert row count
void assert_row_count(ProcedureContext& ctx, uint64_t count) {
    ctx.with_tx([count](TxContext& tx) {
        uint64_t actual = tx.db[my_table].count();
        if (actual != count) {
            LOG_PANIC("Expected " + std::to_string(count) + " rows but got " + std::to_string(actual));
        }
    });
}

// Test transaction that commits
SPACETIMEDB_PROCEDURE(Unit, insert_with_tx_commit, ProcedureContext ctx) {
    // Insert a row and commit
    ctx.with_tx(insert_my_table);
    
    // Assert that there's a row
    assert_row_count(ctx, 1);
    
    return Unit{};
}

// Test transaction that rolls back
SPACETIMEDB_PROCEDURE(Unit, insert_with_tx_rollback, ProcedureContext ctx) {
    // Use try_with_tx and return false to rollback
    ctx.try_with_tx([](TxContext& tx) -> bool {
        insert_my_table(tx);
        return false;  // Rollback
    });
    
    // Assert that there's not a row
    assert_row_count(ctx, 0);
    
    return Unit{};
}

#endif // SPACETIMEDB_UNSTABLE_FEATURES

// ============================================================================
// NOTE: HTTP and Scheduled Procedure tests are excluded
// ============================================================================
//
// The following tests from the Rust version are NOT included yet:
//
// - read_my_schema (requires HTTP support - Part 4)
// - invalid_request (requires HTTP support - Part 4)
// - schedule_proc (requires scheduled procedures - Part 3)
// - scheduled_proc (requires scheduled procedures - Part 3)
//
// These will be added in future parts as the features are implemented.
// ============================================================================
