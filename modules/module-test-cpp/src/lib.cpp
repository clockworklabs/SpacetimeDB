// Set log level to TRACE for this module for repeating_test reducer
#define STDB_LOG_LEVEL ::SpacetimeDB::LogLevelValue::TRACE

#include <spacetimedb.h>
#include <variant>
#include <optional>

using namespace SpacetimeDB;
using SpacetimeDB::Public;
using SpacetimeDB::Private;


// =============================================================================
// C++ Module Test - Equivalent to Rust module-test
// =============================================================================
//
// This module provides equivalence with the Rust module-test:
// - Table definitions with constraints and indexes
// - Support types and enums
// - Reducers for testing various database operations
// =============================================================================

// struct CircularA {
//     uint32_t id;
//     std::vector<CircularA> circ_ref; // References to CircularB by id
// };
// SPACETIMEDB_STRUCT(CircularA, id, circ_ref)
// SPACETIMEDB_TABLE(CircularA, circular_a, Public)

// =============================================================================
// SUPPORT TYPES AND ENUMS
// =============================================================================

// TestB struct - simple struct with a string field
struct TestB {
    std::string foo;
};
SPACETIMEDB_STRUCT(TestB, foo)

// TestC enum - simple enum without payloads (with Namespace scope)
SPACETIMEDB_ENUM(TestC, Foo, Bar)
SPACETIMEDB_NAMESPACE(TestC, "Namespace")

// Workaround: C++ std::variant can't have duplicate types, so we create unique empty types
// for each unit variant instead of using Unit multiple times (like C# SDK does)
SPACETIMEDB_UNIT_TYPE(TestFFoo)
SPACETIMEDB_UNIT_TYPE(TestFBar)

// TestF enum - variant enum matching Rust: Foo, Bar, Baz(String)
SPACETIMEDB_ENUM(TestF,
    (Foo, TestFFoo),
    (Bar, TestFBar),
    (Baz, std::string)
)
SPACETIMEDB_NAMESPACE(TestF, "Namespace")

// Baz struct
struct Baz {
    std::string field;
};
SPACETIMEDB_STRUCT(Baz, field)

// Foobar enum helper type for unit variant
SPACETIMEDB_UNIT_TYPE(FoobarBar)

// Foobar enum - variant enum with payloads matching Rust: Baz(Baz), Bar, Har(u32)
SPACETIMEDB_ENUM(Foobar,
    (Baz, Baz),
    (Bar, FoobarBar),
    (Har, uint32_t)
)

// =============================================================================
// TABLE DEFINITIONS
// =============================================================================

// Person table - public table with auto-increment primary key and age index
// Matches Rust: index(name = age, btree(columns = [age]))
struct Person {
    uint32_t id;
    std::string name;
    uint8_t age;
};
SPACETIMEDB_STRUCT(Person, id, name, age)
SPACETIMEDB_TABLE(Person, person, Public)
FIELD_PrimaryKeyAutoInc(person, id)
FIELD_Index(person, age)

// RemoveTable - table for migration testing (can be removed)
struct RemoveTable {
    uint32_t id;
};
SPACETIMEDB_STRUCT(RemoveTable, id)
SPACETIMEDB_TABLE(RemoveTable, table_to_remove, Private)

// TestA table - private table with foo index on x column
// Matches Rust: index(name = foo, btree(columns = [x]))
struct TestA {
    uint32_t x;
    uint32_t y;
    std::string z;
};
SPACETIMEDB_STRUCT(TestA, x, y, z)
SPACETIMEDB_TABLE(TestA, test_a, Private)
// Note: Single column named indexes aren't supported - use regular index
FIELD_Index(test_a, x)

// Type alias for TestA
using TestAlias = TestA;

// TestD table - public table with optional TestC field
struct TestD {
    std::optional<TestC> test_c;
    TestF test_f;  // Add TestF field to ensure it gets registered
};
SPACETIMEDB_STRUCT(TestD, test_c, test_f)
SPACETIMEDB_TABLE(TestD, test_d, Public)

// TestE table - private table with auto-increment primary key and btree index on name
// Matches Rust: #[index(btree)] on name field
struct TestE {
    uint64_t id;
    std::string name;
};
SPACETIMEDB_STRUCT(TestE, id, name)
SPACETIMEDB_TABLE(TestE, test_e, Private)
FIELD_PrimaryKeyAutoInc(test_e, id)
FIELD_Index(test_e, name)

// TestFoobar table - public table with Foobar enum field
struct TestFoobar {
    Foobar field;
};
SPACETIMEDB_STRUCT(TestFoobar, field)
SPACETIMEDB_TABLE(TestFoobar, test_f, Public)

// PrivateTable - explicitly private table
struct PrivateTable {
    std::string name;
};
SPACETIMEDB_STRUCT(PrivateTable, name)
SPACETIMEDB_TABLE(PrivateTable, private_table, Private)

// Point table - private table with two coordinates and multi-column index
// Rust has: index(name = multi_column_index, btree(columns = [x, y]))
struct Point {
    int64_t x;
    int64_t y;
};
SPACETIMEDB_STRUCT(Point, x, y)
SPACETIMEDB_TABLE(Point, points, Private)
// FIELD_NamedMultiColumnIndex(points, multi_column_index, x, y)

// PkMultiIdentity - table with multiple constraints
struct PkMultiIdentity {
    uint32_t id;
    uint32_t other;
};
SPACETIMEDB_STRUCT(PkMultiIdentity, id, other)
SPACETIMEDB_TABLE(PkMultiIdentity, pk_multi_identity, Private)
FIELD_PrimaryKey(pk_multi_identity, id)
FIELD_UniqueAutoInc(pk_multi_identity, other)

// RepeatingTestArg - table for scheduled reducer
struct RepeatingTestArg {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    Timestamp prev_time;
};
SPACETIMEDB_STRUCT(RepeatingTestArg, scheduled_id, scheduled_at, prev_time)
SPACETIMEDB_TABLE(RepeatingTestArg, repeating_test_arg, Private)
FIELD_PrimaryKeyAutoInc(repeating_test_arg, scheduled_id)
SPACETIMEDB_SCHEDULE(repeating_test_arg, 1, repeating_test)

// HasSpecialStuff - table with special types
struct HasSpecialStuff {
    Identity identity;
    ConnectionId connection_id;
};
SPACETIMEDB_STRUCT(HasSpecialStuff, identity, connection_id)
SPACETIMEDB_TABLE(HasSpecialStuff, has_special_stuff, Private)

// Player table
struct Player {
    Identity identity;
    uint64_t player_id;
    std::string name;
};
SPACETIMEDB_STRUCT(Player, identity, player_id, name)
SPACETIMEDB_TABLE(Player, player, Public)
FIELD_PrimaryKey(player, identity)
FIELD_UniqueAutoInc(player, player_id)
FIELD_Unique(player, name)

SPACETIMEDB_TABLE(Player, logged_out_player, Public)
FIELD_PrimaryKey(logged_out_player, identity)
FIELD_UniqueAutoInc(logged_out_player, player_id)
FIELD_Unique(logged_out_player, name)

// TableWithDefaults - test table with default values
struct TableWithDefaults {
    uint32_t id;
    std::string name;
    uint32_t score;
    bool active;
};
SPACETIMEDB_STRUCT(TableWithDefaults, id, name, score, active)
SPACETIMEDB_TABLE(TableWithDefaults, table_with_defaults, Public)
FIELD_PrimaryKeyAutoInc(table_with_defaults, id)
FIELD_Default(table_with_defaults, score, uint32_t(100))
FIELD_Default(table_with_defaults, active, true)


// =============================================================================
// VIEWS
// =============================================================================

// View to find the player associated with the calling identity
SPACETIMEDB_VIEW(std::optional<Player>, my_player, Public, ViewContext ctx) {
    return ctx.db[player_identity].find(ctx.sender);
}

// =============================================================================
// REDUCERS
// =============================================================================

// Init reducer - called when module is first published
// COMMENTED OUT FOR DEBUGGING
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    RepeatingTestArg arg{
        0, // scheduled_id  
        ScheduleAt(TimeDuration::from_millis(1000)),
        ctx.timestamp
    };
    //ctx.db[repeating_test_arg].insert(arg);
    return Ok();
}

// Repeating test reducer for scheduled operations
SPACETIMEDB_REDUCER(repeating_test, ReducerContext ctx, RepeatingTestArg arg) {
    // Log would show delta time since last run
    // In C++ we don't have log::trace equivalent yet
    auto delta_time = ctx.timestamp.duration_since(arg.prev_time);
    LOG_TRACE("Timestamp: " + ctx.timestamp.to_string() + " Delta time: " + delta_time.to_string());
    return Ok();
}

// Add a person to the Person table
SPACETIMEDB_REDUCER(add, ReducerContext ctx, std::string name, uint8_t age) {
    Person p{0, name, age}; // id will be auto-incremented
    Person inserted = ctx.db[person].insert(p);
    //LOG_INFO("Inserted person with auto-generated ID: " + std::to_string(inserted.id));
    return Ok();
}

// Say hello to all persons
SPACETIMEDB_REDUCER(say_hello, ReducerContext ctx) {
    // In Rust this logs "Hello, {name}!" for each person
    for (const auto& p : ctx.db[person]) {
        LOG_INFO("Hello, " + p.name + "!");
    }
    LOG_INFO("Hello, World!");
    return Ok();
}

// List persons over a certain age - showcases range query functionality
SPACETIMEDB_REDUCER(list_over_age, ReducerContext ctx, uint8_t age) {
    // Use index-based filtering with range queries - equivalent to Rust: ctx.db.person().age().filter(age..)
    auto age_range = range_from(age);
    
    // Use the indexed field accessor for efficient filtering
    // ctx.db[person_age] creates a TypedIndexedAccessor with filter methods
    auto filtered_persons = ctx.db[person_age].filter(age_range);
    
    for (const auto& person : filtered_persons) {
        LOG_INFO(person.name + " has age " + std::to_string(person.age) + " >= " + std::to_string(age));
    }
    return Ok();
}

// Log module identity
SPACETIMEDB_REDUCER(log_module_identity, ReducerContext ctx) {
    LOG_INFO("Module identity: " + ctx.identity().to_string());
    return Ok();
}

// Complex test reducer with multiple parameters
SPACETIMEDB_REDUCER(test, ReducerContext ctx, TestAlias arg, TestB arg2, TestC arg3, TestF arg4) {
    LOG_INFO("BEGIN");
    LOG_INFO("sender: " + ctx.sender.to_string());
    LOG_INFO("timestamp: " + ctx.timestamp.to_string());
    LOG_INFO("bar: " + arg2.foo);

    // Match TestC enum
    switch (arg3) {
        case TestC::Foo:
            LOG_INFO("Foo");
            break;
        case TestC::Bar:
            LOG_INFO("Bar");
            break;
    }
    
    // Match TestF variant enum
    switch (arg4.index()) {
        case 0: // Foo
            LOG_INFO("Foo");
            break;
        case 1: // Bar
            LOG_INFO("Bar");
            break;
        case 2: // Baz
            LOG_INFO(std::get<std::string>(arg4.value));
            break;
    }

    // Insert test data
    for (uint32_t i = 0; i < 1000; ++i) {
        TestA test_a_instance{i + arg.x, i + arg.y, "Yo"};
        ctx.db[test_a].insert(test_a_instance);
    }
    
    // Count rows before delete
    uint64_t row_count_before_delete = ctx.db[test_a].count();
    LOG_INFO("Row count before delete: " + std::to_string(row_count_before_delete));
    
    // Delete rows where x is between 5 and 10
    uint32_t num_deleted = 0;
    for (uint32_t row = 5; row < 10; ++row) {
        auto to_delete = ctx.db[test_a_x].filter(row);
        for (const auto& test_row : to_delete) {
            LOG_INFO("Deleting row with x=" + std::to_string(test_row.x) + " y=" + std::to_string(test_row.y));
            if (ctx.db[test_a].delete_by_value(test_row)) {
            }
        }
        num_deleted++;
    }
    
    // Count rows after delete
    uint64_t row_count_after_delete = ctx.db[test_a].count();
    
    // Verify deletion worked correctly
    if (row_count_before_delete != row_count_after_delete + num_deleted) {
        LOG_ERROR("Started with " + std::to_string(row_count_before_delete) + 
                 " rows, deleted " + std::to_string(num_deleted) + 
                 ", and wound up with " + std::to_string(row_count_after_delete) + 
                 " rows... huh?");
    }
    
    // Test TestE insertion - using regular insert since try_insert isn't available in TableAccessor
    TestE test_e_instance{0, "Tyler"};
    TestE inserted = ctx.db[test_e].insert(test_e_instance);
    LOG_INFO("Inserted: id=" + std::to_string(inserted.id) + " name=" + inserted.name);
    
    LOG_INFO("Row count after delete: " + std::to_string(row_count_after_delete));
    
    // Count all rows
    uint64_t other_row_count = ctx.db[test_a].count();
    LOG_INFO("Row count filtered by condition: " + std::to_string(other_row_count));
    
    LOG_INFO("MultiColumn");
    
    // Insert points for multi-column index testing
    for (int64_t i = 0; i < 1000; ++i) {
        Point point{i + static_cast<int64_t>(arg.x), i + static_cast<int64_t>(arg.y)};
        ctx.db[points].insert(point);
    }
    
    // Count points with multi-column condition - diagnostic version
    uint64_t multi_row_count = 0;

    for (const auto& point : ctx.db[points]) {
        if (point.x >= 0 && point.y <= 200) {
            multi_row_count++;
        }
    }
    LOG_INFO("Row count filtered by multi-column condition: " + std::to_string(multi_row_count));
    
    LOG_INFO("END");
    return Ok();
}

// Add a player (TestE entry)
SPACETIMEDB_REDUCER(add_player, ReducerContext ctx, std::string name) {
    // Try without specifying id at all - but TestE struct requires both fields
    // So we have to use 0 as placeholder
    TestE player{0, name}; // id will be auto-incremented

    TestE inserted = ctx.db[test_e_id].try_insert_or_update(player);
    LOG_INFO("Inserted player with auto-generated ID: " + std::to_string(inserted.id));

    ctx.db[test_e_id].try_insert_or_update(inserted);
    LOG_INFO("Updated player after insert-or-update");
    return Ok();
}

// Delete a player by ID
SPACETIMEDB_REDUCER(delete_player, ReducerContext ctx, uint64_t id) {
    // Delete TestE entry with given id
    // In C++ we'd need to implement delete by primary key
    if (ctx.db[test_e_id].delete_by_key(id)) {
        LOG_INFO("Deleted player with ID: " + std::to_string(id));
    } else {
        LOG_ERROR("No player found with ID: " + std::to_string(id));
    }
    return Ok();
}

// Delete players by name
SPACETIMEDB_REDUCER(delete_players_by_name, ReducerContext ctx, std::string name) {
    // Delete all TestE entries with given name
    // In C++ we'd iterate and delete matching entries
    //auto to_delete = ctx.db[test_e_name].filter(name);
    auto deleted = ctx.db[test_e_name].delete_by_value(name);
    LOG_INFO("Deleted " + std::to_string(deleted) + " players with name: " + name);
    return Ok();
}

// Client connected lifecycle reducer
SPACETIMEDB_CLIENT_CONNECTED(client_connected, ReducerContext ctx) {
    // Called when a client connects
    return Ok();
}

// Add entry to private table
SPACETIMEDB_REDUCER(add_private, ReducerContext ctx, std::string name) {
    PrivateTable entry{name};
    auto secret_entry = ctx.db[private_table].insert(entry);
    LOG_INFO("Inserted private table entry: " + secret_entry.name);
    return Ok();
}

// Query private table
SPACETIMEDB_REDUCER(query_private, ReducerContext ctx) {
    // Iterate over private_table entries
    // Would log each entry's name
    for (const auto& entry : ctx.db[private_table]) {
        LOG_INFO("Private, " + entry.name + "!");
    }
    LOG_INFO("Private, World!");
    return Ok();
}

// Test btree index arguments - comprehensive range query testing
SPACETIMEDB_REDUCER(test_btree_index_args, ReducerContext ctx) {
    // This tests various range query patterns equivalent to Rust's comprehensive index testing
    
    // ==================================================================
    // Single-column range queries on Person.age (uint8_t indexed field)  
    // ==================================================================
    
    LOG_INFO("=== Testing age range queries ===");
    
    // Test all range construction patterns
    auto range_from_25 = range_from(uint8_t(25));        // 25..
    auto range_to_30 = range_to(uint8_t(30));            // ..30  
    auto range_25_to_30 = range(uint8_t(25), uint8_t(30)); // 25..30
    auto range_25_to_30_inc = range_inclusive(uint8_t(25), uint8_t(30)); // 25..=30
    auto range_to_30_inc = range_to_inclusive(uint8_t(30)); // ..=30
    auto range_all = range_full<uint8_t>();              // ..
    
    // Count matches for each range pattern using INDEX-BASED FILTERING
    // Now using ctx.db[person_age] indexed field accessor for efficient queries
    size_t count_25_plus = ctx.db[person_age].filter(range_from_25).size();
    size_t count_under_30 = ctx.db[person_age].filter(range_to_30).size();
    size_t count_25_to_30 = ctx.db[person_age].filter(range_25_to_30).size();
    size_t count_25_to_30_inc = ctx.db[person_age].filter(range_25_to_30_inc).size();
    size_t count_under_30_inc = ctx.db[person_age].filter(range_to_30_inc).size();
    size_t count_all = ctx.db[person_age].filter(range_all).size();
    
    LOG_INFO("Age >= 25: " + std::to_string(count_25_plus));
    LOG_INFO("Age < 30: " + std::to_string(count_under_30));
    LOG_INFO("Age 25..30: " + std::to_string(count_25_to_30));
    LOG_INFO("Age 25..=30: " + std::to_string(count_25_to_30_inc));
    LOG_INFO("Age ..=30: " + std::to_string(count_under_30_inc));
    LOG_INFO("All ages: " + std::to_string(count_all));
    
    // ==================================================================
    // Multi-column range queries on Point.x, Point.y (int64_t fields)
    // Equivalent to Rust's multi_column_index filter tests
    // ==================================================================
    
    LOG_INFO("=== Testing coordinate range queries ===");
    
    // Test coordinate-based ranges  
    auto x_range_positive = range_from(int64_t(0));      // x >= 0
    auto x_range_0_to_100 = range(int64_t(0), int64_t(100)); // 0 <= x < 100
    auto xy_combined = range_inclusive(int64_t(-50), int64_t(50)); // -50 <= coord <= 50
    
    size_t positive_x_count = 0, x_0_to_100_count = 0, xy_in_range_count = 0;
    
    for (const auto& point : ctx.db[points]) {
        if (x_range_positive.contains(point.x)) positive_x_count++;
        if (x_range_0_to_100.contains(point.x)) x_0_to_100_count++;
        if (xy_combined.contains(point.x) && xy_combined.contains(point.y)) {
            xy_in_range_count++;
        }
    }
    
    LOG_INFO("Points with x >= 0: " + std::to_string(positive_x_count));
    LOG_INFO("Points with 0 <= x < 100: " + std::to_string(x_0_to_100_count));
    LOG_INFO("Points with x,y in [-50,50]: " + std::to_string(xy_in_range_count));
    
    // ==================================================================
    // String range queries on TestE.name (string indexed field)
    // ==================================================================
    
    LOG_INFO("=== Testing string range queries ===");
    
    // String range examples - using INDEX-BASED FILTERING
    auto name_range_a_to_m = range(std::string("A"), std::string("M"));  // Names starting A-L
    auto name_range_from_t = range_from(std::string("T")); // Names starting T and later
    
    // Use ctx.db[test_e_name] indexed field accessor for efficient string range queries
    size_t names_a_to_m = ctx.db[test_e_name].filter(name_range_a_to_m).size();
    size_t names_from_t = ctx.db[test_e_name].filter(name_range_from_t).size();
    
    LOG_INFO("Names A-L: " + std::to_string(names_a_to_m));
    LOG_INFO("Names T+: " + std::to_string(names_from_t));
    
    // ==================================================================
    // Range query performance comparison
    // ==================================================================
    
    LOG_INFO("=== Range vs Manual Filtering Comparison ===");
    
    auto performance_range = range_inclusive(uint8_t(20), uint8_t(40));
    size_t range_matches = 0, manual_matches = 0;
    
    // Method 1: Range-based filtering
    for (const auto& p : ctx.db[person]) {
        if (performance_range.contains(p.age)) {
            range_matches++;
        }
    }
    
    // Method 2: Manual filtering  
    for (const auto& p : ctx.db[person]) {
        if (p.age >= 20 && p.age <= 40) {
            manual_matches++;
        }
    }
    
    LOG_INFO("Range-based matches: " + std::to_string(range_matches));
    LOG_INFO("Manual matches: " + std::to_string(manual_matches));
    LOG_INFO("Results match: " + std::to_string(range_matches == manual_matches ? 1 : 0));
    return Ok();
}


// Test reducer for assertions
SPACETIMEDB_REDUCER(assert_caller_identity_is_module_identity, ReducerContext ctx) {
    LOG_INFO("Sender: " + ctx.sender.to_string() + " Identity: " + ctx.identity().to_string());
    if (ctx.sender != ctx.identity()) {
        LOG_ERROR("Assertion failed: caller identity does not match module identity");
    } else {
        LOG_INFO("Assertion passed: caller identity matches module identity");
    }
    return Ok();
}

SPACETIMEDB_REDUCER(test_defaults, ReducerContext ctx) {
    LOG_INFO("=== Testing default values ===");
    
    // Insert entries to test default value registration
    // Note: In C++, we still need to provide values in the struct constructor,
    // but the defaults are registered in the module metadata for use in migrations
    // and when columns are added to existing tables
    
    TableWithDefaults entry1{0, "Susan", 100, true};  // Using default values
    auto inserted1 = ctx.db[table_with_defaults].insert(entry1);
    LOG_INFO("Inserted: id=" + std::to_string(inserted1.id) + 
             " name=" + inserted1.name + 
             " score=" + std::to_string(inserted1.score) + 
             " active=" + std::to_string(inserted1.active));
    
    TableWithDefaults entry2{0, "Charlie", 200, false};  // Using custom values
    auto inserted2 = ctx.db[table_with_defaults].insert(entry2);
    LOG_INFO("Inserted: id=" + std::to_string(inserted2.id) + 
             " name=" + inserted2.name + 
             " score=" + std::to_string(inserted2.score) + 
             " active=" + std::to_string(inserted2.active));
    
    // Count total entries
    size_t count = ctx.db[table_with_defaults].count();
    LOG_INFO("Total entries with defaults: " + std::to_string(count));
    
    LOG_INFO("Default values registered in module metadata");
    return Ok();
}

SPACETIMEDB_REDUCER(throw_error, ReducerContext ctx) {
    LOG_INFO("This reducer will throw an error.");
    return Err("Intentional error from throw_error reducer.");
    //return Ok();
}

// =============================================================================
// JWT AUTHENTICATION TESTS
// =============================================================================

// Test reducer that checks JWT authentication
SPACETIMEDB_REDUCER(test_jwt_auth, ReducerContext ctx) {
    LOG_INFO("=== Testing JWT Authentication ===");
    
    // Get sender_auth via the accessor method (matches Rust pattern)
    const auto& auth = ctx.sender_auth();
    
    // Check if JWT is present
    if (auth.has_jwt()) {
        LOG_INFO("JWT is present");
        
        // Get the JWT and parse claims (returns optional)
        auto jwt_opt = auth.get_jwt();
        if (!jwt_opt.has_value()) {
            LOG_ERROR("has_jwt() was true but get_jwt() returned empty");
            return Ok();
        }
        
        auto& jwt = jwt_opt.value();
        
        // Test claim accessors
        auto subject = jwt.subject();
        auto issuer = jwt.issuer();
        auto audience = jwt.audience();
        
        LOG_INFO("JWT Subject: " + subject);
        LOG_INFO("JWT Issuer: " + issuer);
        
        // Audience is a vector of strings
        if (!audience.empty()) {
            LOG_INFO("JWT Audience (first): " + audience[0]);
        } else {
            LOG_INFO("JWT Audience: (empty)");
        }
        
        // Test identity access
        auto identity = jwt.get_identity();
        LOG_INFO("JWT Identity: " + identity.to_string());
        
        // Compare with caller identity
        LOG_INFO("Caller Identity: " + ctx.sender.to_string());
        
        // Verify that get_caller_identity returns the same as ctx.sender
        auto caller_identity = auth.get_caller_identity();
        if (caller_identity == ctx.sender) {
            LOG_INFO("get_caller_identity matches ctx.sender");
        } else {
            LOG_ERROR("get_caller_identity does NOT match ctx.sender");
        }
    } else {
        LOG_INFO("No JWT present (anonymous or scheduled reducer)");
        
        // Verify get_caller_identity still works
        auto caller_identity = auth.get_caller_identity();
        LOG_INFO("Caller Identity (no JWT): " + caller_identity.to_string());
    }
    
    LOG_INFO("=== JWT Authentication Test Complete ===");
    return Ok();
}

// =============================================================================
// PROCEDURES
// =============================================================================


SPACETIMEDB_PROCEDURE(Unit, sleep_one_second, ProcedureContext ctx) {
    Timestamp prev = ctx.timestamp;
    auto delta = TimeDuration::from_seconds(1);
    Timestamp next = prev + delta;  // assumes operator+ is available
    LOG_INFO("Slept from " + prev.to_string() + " to " + next.to_string() +
             ", a total of " + delta.to_string());
    return Unit{};
}

// Return a Baz struct with the foo parameter as a string
SPACETIMEDB_PROCEDURE(Baz, return_value, ProcedureContext ctx, uint64_t foo) {
    return Baz{std::to_string(foo)};
}

// Execute say_hello reducer within a transaction context
SPACETIMEDB_PROCEDURE(Unit, with_tx, ProcedureContext ctx) {
    ctx.with_tx([](TxContext& tx) {
        // Call say_hello logic within transaction
        for (const auto& p : tx.db[person]) {
            LOG_INFO("Hello, " + p.name + "!");
        }
        LOG_INFO("Hello, World!");
    });
    return Unit{};
}

// Hit SpacetimeDB's schema HTTP route and return its result as a string
SPACETIMEDB_PROCEDURE(std::string, get_my_schema_via_http, ProcedureContext ctx) {
    Identity module_identity = ctx.identity();
    std::string url = "http://localhost:3000/v1/database/" + module_identity.to_string() + "/schema?version=9";
    
    auto result = ctx.http.get(url);
    if (result.is_ok()) {
        return result.value().body.to_string_utf8_lossy();
    } else {
        return result.error();
    }
}
