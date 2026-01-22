#include "spacetimedb.h"

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

// Test UUID v4 generation
SPACETIMEDB_PROCEDURE(std::string, test_uuid_v4, ProcedureContext ctx) {
    Uuid uuid = ctx.new_uuid_v4();
    
    // Verify it's a valid v4 UUID
    auto version = uuid.get_version();
    if (!version.has_value() || version.value() != Uuid::Version::V4) {
        LOG_PANIC("Generated UUID is not v4");
    }
    
    std::string uuid_str = uuid.to_string();
    LOG_INFO("Generated UUID v4: " + uuid_str);
    return uuid_str;
}

// Test UUID v7 generation
SPACETIMEDB_PROCEDURE(std::string, test_uuid_v7, ProcedureContext ctx) {
    Uuid uuid = ctx.new_uuid_v7();
    
    // Verify it's a valid v7 UUID
    auto version = uuid.get_version();
    if (!version.has_value() || version.value() != Uuid::Version::V7) {
        LOG_PANIC("Generated UUID is not v7");
    }
    
    std::string uuid_str = uuid.to_string();
    LOG_INFO("Generated UUID v7: " + uuid_str);
    return uuid_str;
}

// Test UUID string round-trip
SPACETIMEDB_PROCEDURE(std::string, test_uuid_round_trip, ProcedureContext ctx) {
    // Test with NIL
    std::string nil_str = Uuid::nil().to_string();
    auto nil_parsed = Uuid::parse_str(nil_str);
    if (!nil_parsed.has_value() || nil_parsed.value() != Uuid::nil()) {
        LOG_PANIC("NIL UUID round-trip failed");
    }
    
    // Test with MAX
    std::string max_str = Uuid::max().to_string();
    auto max_parsed = Uuid::parse_str(max_str);
    if (!max_parsed.has_value() || max_parsed.value() != Uuid::max()) {
        LOG_PANIC("MAX UUID round-trip failed");
    }
    
    // Test with generated UUID
    Uuid uuid = ctx.new_uuid_v7();
    std::string uuid_str = uuid.to_string();
    auto uuid_parsed = Uuid::parse_str(uuid_str);
    if (!uuid_parsed.has_value() || uuid_parsed.value() != uuid) {
        LOG_PANIC("Generated UUID round-trip failed");
    }
    
    LOG_INFO("All UUID round-trips passed");
    return "NIL: " + nil_str + ", MAX: " + max_str + ", V7: " + uuid_str;
}

// Test UUID version detection
SPACETIMEDB_PROCEDURE(std::string, test_uuid_versions, ProcedureContext ctx) {
    // Debug: Check what MAX actually is
    LOG_INFO("MAX as u128: high=" + std::to_string(Uuid::max().as_u128().high) + ", low=" + std::to_string(Uuid::max().as_u128().low));
    LOG_INFO("MAX.to_string(): " + Uuid::max().to_string());
    
    // Test NIL
    auto nil_version = Uuid::nil().get_version();
    if (!nil_version.has_value() || nil_version.value() != Uuid::Version::Nil) {
        LOG_PANIC("NIL version check failed");
    }
    
    // Test MAX
    auto max_version = Uuid::max().get_version();
    if (!max_version.has_value() || max_version.value() != Uuid::Version::Max) {
        LOG_PANIC("MAX version check failed");
    }
    
    // Test V4
    Uuid uuid_v4 = ctx.new_uuid_v4();
    auto v4_version = uuid_v4.get_version();
    if (!v4_version.has_value() || v4_version.value() != Uuid::Version::V4) {
        LOG_PANIC("V4 version check failed");
    }
    
    // Test V7
    Uuid uuid_v7 = ctx.new_uuid_v7();
    auto v7_version = uuid_v7.get_version();
    if (!v7_version.has_value() || v7_version.value() != Uuid::Version::V7) {
        LOG_PANIC("V7 version check failed");
    }
    
    LOG_INFO("All UUID version checks passed");
    return "NIL, MAX, V4, and V7 versions detected correctly";
}

// Test UUID ordering
SPACETIMEDB_PROCEDURE(std::string, test_uuid_ordering, ProcedureContext ctx) {
    Uuid u1 = Uuid::from_u128(u128(0, 1));
    Uuid u2 = Uuid::from_u128(u128(0, 2));
    
    if (!(u1 < u2)) {
        LOG_PANIC("UUID ordering failed: u1 < u2");
    }
    if (!(u2 > u1)) {
        LOG_PANIC("UUID ordering failed: u2 > u1");
    }
    if (!(u1 <= u1)) {
        LOG_PANIC("UUID ordering failed: u1 <= u1");
    }
    if (!(u1 == u1)) {
        LOG_PANIC("UUID ordering failed: u1 == u1");
    }
    if (!(u1 != u2)) {
        LOG_PANIC("UUID ordering failed: u1 != u2");
    }
    
    LOG_INFO("All UUID ordering checks passed");
    return "UUID comparison operators work correctly";
}

// Test UUID counter extraction
SPACETIMEDB_PROCEDURE(std::string, test_uuid_counter, ProcedureContext ctx) {
    // Generate multiple UUIDs and verify counter increments
    std::vector<int32_t> counters;
    
    for (int i = 0; i < 10; i++) {
        Uuid uuid = ctx.new_uuid_v7();
        int32_t counter = uuid.get_counter();
        counters.push_back(counter);
    }
    
    // Verify counters are increasing
    for (size_t i = 1; i < counters.size(); i++) {
        if (counters[i] <= counters[i-1]) {
            LOG_PANIC("Counter not incrementing: " + std::to_string(counters[i-1]) + " >= " + std::to_string(counters[i]));
        }
    }
    
    std::string result = "Counters: ";
    for (size_t i = 0; i < counters.size(); i++) {
        result += std::to_string(counters[i]);
        if (i < counters.size() - 1) result += ", ";
    }
    
    LOG_INFO("Counter extraction test passed: " + result);
    return result;
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
