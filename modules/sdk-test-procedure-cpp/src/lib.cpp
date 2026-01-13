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
// Procedure Tests - Part 4: HTTP Requests
// ============================================================================
#ifdef SPACETIMEDB_UNSTABLE_FEATURES

// Test HTTP GET request to the module's own schema endpoint
SPACETIMEDB_PROCEDURE(std::string, read_my_schema, ProcedureContext ctx) {
    // Get the module identity (database address)
    Identity module_identity = ctx.identity();
    std::string identity_hex = module_identity.to_hex_string();
    
    LOG_INFO("read_my_schema using identity: " + identity_hex);
    
    // Make HTTP GET request to the schema endpoint (matches Rust)
    std::string url = "http://localhost:3000/v1/database/" + identity_hex + "/schema?version=9";
    auto result = ctx.http.Get(url);
    
    if (!result.is_ok()) {
        LOG_INFO("read_my_schema error: " + result.error());
        LOG_PANIC(result.error());
        return ""; // Never reached
    }
    
    auto& response = result.value();
    std::string body = response.body.ToStringUtf8Lossy();
    
    LOG_INFO("read_my_schema status: " + std::to_string(response.status_code) + ", body length: " + std::to_string(body.length()));
    
    return body;
}

// Test HTTP request with invalid URL (should fail gracefully)
SPACETIMEDB_PROCEDURE(std::string, invalid_request, ProcedureContext ctx) {
    auto result = ctx.http.Get("http://foo.invalid/");
    
    if (result.is_ok()) {
        // Unexpected success - panic like Rust version
        auto& response = result.value();
        std::string body = response.body.ToStringUtf8Lossy();
        LOG_INFO("invalid_request unexpected success: " + body);
        LOG_PANIC("Got result from requesting `http://foo.invalid`... huh?\n" + body);
        return ""; // Never reached
    }
    
    std::string error = result.error();
    LOG_INFO("invalid_request expected error: " + error);
    return error;
}

// Test HTTP GET request to a simple JSON endpoint
SPACETIMEDB_PROCEDURE(std::string, test_simple_http, ProcedureContext ctx) {
    // Use httpbin.org which returns simple JSON responses
    auto result = ctx.http.Get("https://httpbin.org/get");
    
    if (!result.is_ok()) {
        LOG_INFO("test_simple_http error: " + result.error());
        return "Error: " + result.error();
    }
    
    auto& response = result.value();
    std::string body = response.body.ToStringUtf8Lossy();
    
    LOG_INFO("test_simple_http status: " + std::to_string(response.status_code));
    LOG_INFO("test_simple_http headers count: " + std::to_string(response.headers.size()));
    LOG_INFO("test_simple_http body preview (first 100 chars): " + body.substr(0, std::min(size_t(100), body.length())));
    
    // Return a summary
    return "Status: " + std::to_string(response.status_code) + ", Body length: " + std::to_string(body.length()) + " bytes";
}

#endif // SPACETIMEDB_UNSTABLE_FEATURES

// ============================================================================
// NOTE: Scheduled Procedure tests are excluded
// ============================================================================
//
// The following tests from the Rust version are NOT included yet:
//
// - schedule_proc (requires scheduled procedures - Part 3)
// - scheduled_proc (requires scheduled procedures - Part 3)
//
// These will be added in Part 3 as the feature is implemented.
// ============================================================================
