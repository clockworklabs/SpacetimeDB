#include "spacetimedb.h"

using namespace SpacetimeDB;

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

// Table for UUID ordering tests
struct PkUuid {
    Uuid u;
    uint8_t data;
};
SPACETIMEDB_STRUCT(PkUuid, u, data)
SPACETIMEDB_TABLE(PkUuid, pk_uuid, Public)

// Table for scheduled procedure tests
struct ScheduledProcTable {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    Timestamp reducer_ts;
    uint8_t x;
    uint8_t y;
};
SPACETIMEDB_STRUCT(ScheduledProcTable, scheduled_id, scheduled_at, reducer_ts, x, y)
SPACETIMEDB_TABLE(ScheduledProcTable, scheduled_proc_table, Private)
FIELD_PrimaryKeyAutoInc(scheduled_proc_table, scheduled_id);
SPACETIMEDB_SCHEDULE(scheduled_proc_table, 1, scheduled_proc)  // Column 1 is scheduled_at

// Table for storing procedure results
struct ProcInsertsInto {
    Timestamp reducer_ts;
    Timestamp procedure_ts;
    uint8_t x;
    uint8_t y;
};
SPACETIMEDB_STRUCT(ProcInsertsInto, reducer_ts, procedure_ts, x, y)
SPACETIMEDB_TABLE(ProcInsertsInto, proc_inserts_into, Public)

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
    auto result = ctx.http.get(url);
    
    if (!result.is_ok()) {
        LOG_INFO("read_my_schema error: " + result.error());
        LOG_PANIC(result.error());
        return ""; // Never reached
    }
    
    auto& response = result.value();
    std::string body = response.body.to_string_utf8_lossy();
    
    LOG_INFO("read_my_schema status: " + std::to_string(response.status_code) + ", body length: " + std::to_string(body.length()));
    
    return body;
}

// Test HTTP request with invalid URL (should fail gracefully)
SPACETIMEDB_PROCEDURE(std::string, invalid_request, ProcedureContext ctx) {
    auto result = ctx.http.get("http://foo.invalid/");
    
    if (result.is_ok()) {
        // Unexpected success - panic like Rust version
        auto& response = result.value();
        std::string body = response.body.to_string_utf8_lossy();
        LOG_INFO("invalid_request unexpected success: " + body);
        LOG_PANIC("Got result from requesting `http://foo.invalid`... huh?\n" + body);
        return ""; // Never reached
    }
    
    std::string error = result.error();
    LOG_INFO("invalid_request expected error: " + error);
    return error;
}

#endif // SPACETIMEDB_UNSTABLE_FEATURES

// ============================================================================
// UUID Tests
// ============================================================================

// Test UUID v7 generation and ordering
SPACETIMEDB_PROCEDURE(Unit, sorted_uuids_insert, ProcedureContext ctx) {
    ctx.with_tx([](TxContext& tx) {
        // Generate and insert 1000 UUIDs
        for (int i = 0; i < 1000; i++) {
            Uuid uuid = tx.new_uuid_v7();
            tx.db[pk_uuid].insert(PkUuid{uuid, 0});
        }
        
        // Verify UUIDs are sorted
        std::optional<Uuid> last_uuid;
        for (const auto& row : tx.db[pk_uuid]) {
            if (last_uuid.has_value()) {
                if (last_uuid.value() >= row.u) {
                    LOG_PANIC("UUIDs are not sorted correctly");
                }
            }
            last_uuid = row.u;
        }
        
        LOG_INFO("Successfully inserted and verified 1000 sorted UUIDs");
    });
    
    return Unit{};
}

// ============================================================================
// Scheduled Procedure Tests
// ============================================================================

// Reducer that schedules the scheduled_proc procedure
SPACETIMEDB_REDUCER(schedule_proc, ReducerContext ctx) {
    LOG_INFO("schedule_proc called at timestamp: " + std::to_string(ctx.timestamp.micros_since_epoch()));
    // Schedule the procedure to run in 1s (1000ms = 1,000,000 microseconds)
    ctx.db[scheduled_proc_table].insert(ScheduledProcTable{
        0,  // scheduled_id (auto-incremented)
        ScheduleAt(TimeDuration::from_micros(1000000)),  // 1 second from now
        ctx.timestamp,  // Store the timestamp at which this reducer was called
        42,  // x
        24   // y
    });
    
    return Ok();
}

// Procedure that should be called 1s after schedule_proc
SPACETIMEDB_PROCEDURE(Unit, scheduled_proc, ProcedureContext ctx, ScheduledProcTable data) {
    Timestamp reducer_ts = data.reducer_ts;
    uint8_t x = data.x;
    uint8_t y = data.y;
    Timestamp procedure_ts = ctx.timestamp;
    
    LOG_INFO("scheduled_proc called - procedure_ts: " + std::to_string(procedure_ts.micros_since_epoch()) + 
             ", reducer_ts: " + std::to_string(reducer_ts.micros_since_epoch()));
    
    ctx.with_tx([reducer_ts, procedure_ts, x, y](TxContext& tx) {
        tx.db[proc_inserts_into].insert(ProcInsertsInto{
            reducer_ts,
            procedure_ts,
            x,
            y
        });
    });
    
    return Unit{};
}
