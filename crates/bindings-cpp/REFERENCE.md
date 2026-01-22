# SpacetimeDB C++ Module Reference

Complete API reference for building SpacetimeDB modules in C++.

## Table of Contents

- [Overview](#overview)
- [Setup](#setup)
- [Core Concepts](#core-concepts)
- [Tables](#tables)
- [Reducers](#reducers)
- [Database Operations](#database-operations)
- [Types and Serialization](#types-and-serialization)
- [Constraints and Indexing](#constraints-and-indexing)
- [Special Types](#special-types)
- [Logging](#logging)
- [Build System](#build-system)
- [Examples](#examples)

## Overview

SpacetimeDB allows writing server-side applications in C++ that compile to WebAssembly and run inside the database. This eliminates the traditional application server layer, providing microsecond latency and automatic real-time synchronization to clients.

### Module Structure

A SpacetimeDB C++ module consists of:

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDb;

// 1. Data structures (structs/enums)
struct MyData { /* fields */ };
SPACETIMEDB_STRUCT(MyData, /* field names */);

// 2. Tables (persistent storage)
SPACETIMEDB_TABLE(MyData, my_table, Public);

// 3. Constraints (applied after table registration)
FIELD_PrimaryKey(my_table, id);

// 4. Reducers (functions clients can call)
SPACETIMEDB_REDUCER(my_function, ReducerContext ctx, /* parameters */) {
    // Your logic here
    return Ok();
}
```

## Setup

### Project Initialization

```bash
# Create new project
spacetime init --lang cpp my-module
cd my-module

# Build and publish
emcmake cmake -B build .
cmake --build build
spacetime publish . my-database
```

### Manual Setup

For existing projects, ensure your `CMakeLists.txt` includes:

```cmake
cmake_minimum_required(VERSION 3.16)
project(my-module)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Set path to SpacetimeDB C++ library
set(SPACETIMEDB_CPP_LIBRARY_PATH "path/to/crates/bindings-cpp")

add_executable(lib src/lib.cpp)
target_include_directories(lib PRIVATE ${SPACETIMEDB_CPP_LIBRARY_PATH}/include)

add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} spacetimedb_cpp_library)
target_link_libraries(lib PRIVATE spacetimedb_cpp_library)

# Emscripten settings for WASM
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    set_target_properties(lib PROPERTIES
        SUFFIX ".wasm"
        LINK_FLAGS "-s STANDALONE_WASM=1 ..."
    )
endif()
```

## Core Concepts

### Tables

Tables are persistent data storage defined as C++ structs:

```cpp
struct User {
    uint32_t id;
    std::string name;
    bool active;
};
```

### Reducers

Reducers are functions that modify data and can be called by clients:

```cpp
SPACETIMEDB_REDUCER(create_user, ReducerContext ctx, std::string name) {
    User user{0, name, true};
    ctx.db[users].insert(user);
    return Ok();
}
```

### Transactions

All reducer calls run in transactions. If a reducer fails, all changes are rolled back.

## Tables

### Defining Tables

Tables require two steps: struct registration and table registration.

```cpp
// Step 1: Register struct for serialization
struct Product {
    uint32_t id;
    std::string name;
    double price;
    std::optional<std::string> description;
};
SPACETIMEDB_STRUCT(Product, id, name, price, description);

// Step 2: Register as table
SPACETIMEDB_TABLE(Product, products, Public);
```

### Table Visibility

- **Public**: `SPACETIMEDB_TABLE(Type, name, Public)` - Synced to subscribed clients
- **Private**: `SPACETIMEDB_TABLE(Type, name, Private)` - Only accessible by reducers

### Multiple Tables per Type

The same struct can be used for multiple tables:

```cpp
SPACETIMEDB_STRUCT(LogEntry, timestamp, message, level);
SPACETIMEDB_TABLE(LogEntry, error_logs, Private);
SPACETIMEDB_TABLE(LogEntry, debug_logs, Private);
SPACETIMEDB_TABLE(LogEntry, audit_logs, Public);
```

## Reducers

### Basic Reducers

```cpp
SPACETIMEDB_REDUCER(function_name, ReducerContext ctx, /* parameters */) {
    // Reducer logic
    return Ok();
}
```

**Parameters:**
- First parameter must always be `ReducerContext ctx`
- Additional parameters are passed by clients
- All parameter types must be registered with `SPACETIMEDB_STRUCT`
- Reducer must return `ReducerResult` (Outcome<void>): either `Ok()` on success or `Err(message)` on error

### Lifecycle Reducers

Special reducers called automatically by SpacetimeDB:

```cpp
// Called when module is first published
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    // Initialize data, create default records
    LOG_INFO("Module initialized");
    return Ok();
}

// Called when a client connects
SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) {
    LOG_INFO("Client connected");
    // ctx.sender contains the client's Identity
    return Ok();
}

// Called when a client disconnects
SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) {
    LOG_INFO("Client disconnected");
    // Update user status, cleanup resources
    return Ok();
}
```

### Reducer Context

The `ReducerContext` provides access to:

```cpp
SPACETIMEDB_REDUCER(example, ReducerContext ctx, /* params */) {
    // Database access
    ctx.db[table_name].insert(record);
    
    // Client identity
    Identity client = ctx.sender;
    
    // Current timestamp
    Timestamp now = ctx.timestamp;
    
    // Random number generation
    uint64_t random = ctx.rng().next_u64();
    int dice_roll = ctx.random<int>();
}
```

## Database Operations

### Table Access Patterns

SpacetimeDB C++ uses two access patterns:

#### 1. Table Access (`ctx.db[table_name]`)

For basic operations and iteration:

```cpp
// Insert (basic)
User user{0, "Alice", true};
ctx.db[users].insert(user);

// Insert with auto-increment callback
// If users table has FIELD_PrimaryKeyAutoInc(users, id)
User user_with_autoinc{0, "Bob", true};  // id=0 will be auto-generated
User inserted_user = ctx.db[users].insert(user_with_autoinc);
LOG_INFO("Created user with auto-generated ID: " + std::to_string(inserted_user.id));

// Iterate all rows
for (const auto& user : ctx.db[users]) {
    LOG_INFO("User: " + user.name);
}

// Update (requires full row replacement)
for (auto& user : ctx.db[users]) {
    if (user.name == "Alice") {
        user.active = false;
        ctx.db[users].update(user);
        break;
    }
}
```

#### 2. Field Access (`ctx.db[table_field]`)

For indexed operations (requires constraints):

```cpp
// Table with primary key
SPACETIMEDB_TABLE(User, users, Public);
FIELD_PrimaryKey(users, id);

// Efficient operations using primary key
SPACETIMEDB_REDUCER(delete_user, ReducerContext ctx, uint32_t user_id) {
    ctx.db[users_id].delete_by_key(user_id);
    return Ok();
}

SPACETIMEDB_REDUCER(update_user, ReducerContext ctx, uint32_t user_id, std::string new_name) {
    User updated_user{user_id, new_name, true};
    ctx.db[users_id].update(updated_user);
    return Ok();
}
```

### Supported Operations

| Operation | Table Access | Field Access (Indexed) |
|-----------|--------------|------------------------|
| Insert | `insert(row)` | - |
| Delete | Manual iteration | `delete_by_key(key)` |
| Update | `update(row)` | `update(row)` |
| Query | Iteration | `filter(value)` |

## Random Number Generation

The C++ SDK provides deterministic random number generation through the `ReducerContext`. The RNG is seeded with the reducer's timestamp, ensuring reproducible behavior across SpacetimeDB instances.

### Basic Usage

```cpp
SPACETIMEDB_REDUCER(dice_game, ReducerContext ctx, std::string player, uint32_t sides) {
    // Get the RNG instance (lazily initialized)
    auto& rng = ctx.rng();
    
    // Generate random numbers
    uint32_t dice_roll = rng.gen_range(1u, sides);
    uint64_t large_number = rng.next_u64();
    float probability = rng.gen_float();  // [0, 1)
    bool coin_flip = rng.gen_bool();
    
    // Convenience method for single values
    int random_int = ctx.random<int>();
    double random_double = ctx.random<double>();
    return Ok();
}
```

### Core Methods

#### `ctx.rng()`
Returns the random number generator instance for this reducer call.

#### `ctx.random<T>()`
Convenience method to generate a single random value of type T.

### RNG Methods

#### Basic Generation
```cpp
auto& rng = ctx.rng();

// Generate raw random bits
uint32_t bits32 = rng.next_u32();
uint64_t bits64 = rng.next_u64();

// Generate typed values
int value = rng.gen<int>();
float value = rng.gen<float>();    // [0, 1)
double value = rng.gen<double>();  // [0, 1)
bool value = rng.gen<bool>();
```

#### Range Generation
```cpp
// Integer ranges [min, max] (inclusive)
int dice = rng.gen_range(1, 6);
uint64_t large = rng.gen_range(1000000u, 9999999u);

// Floating point ranges [min, max)
float speed = rng.gen_range(0.5f, 2.0f);
double precision = rng.gen_range(0.0, 100.0);
```

#### Utility Methods
```cpp
// Fill buffer with random bytes
std::vector<uint8_t> buffer(32);
rng.fill_bytes(buffer);

uint8_t raw_buffer[16];
rng.fill_bytes(raw_buffer, 16);

// Shuffle containers
std::vector<std::string> deck = {"Ace", "King", "Queen", "Jack"};
rng.shuffle(deck.begin(), deck.end());

// Sample random element
std::vector<int> options = {10, 20, 30, 40};
int choice = rng.sample(options);  // Returns one of the elements
```

### Deterministic Behavior

The RNG is seeded with the reducer's timestamp in microseconds:
- **Same timestamp = Same sequence**: Reducers called at the exact same microsecond will generate identical random sequences
- **Different timestamps = Different sequences**: Even microsecond differences produce completely different random sequences
- **Reproducible across instances**: The same reducer call will generate the same random values on any SpacetimeDB instance

### Example: Casino Game

```cpp
SPACETIMEDB_TABLE(GameResult, game_results, Public)
SPACETIMEDB_STRUCT(GameResult, player, game_type, result, payout, timestamp)
struct GameResult {
    uint32_t id;
    std::string player;
    std::string game_type;
    std::string result;
    uint32_t payout;
    Timestamp timestamp;
};

SPACETIMEDB_REDUCER(play_slots, ReducerContext ctx, std::string player_name) {
    auto& rng = ctx.rng();
    
    // Generate three slot symbols
    std::vector<std::string> symbols = {"🍒", "🍋", "🔔", "⭐", "💎"};
    std::string slot1 = rng.sample(symbols);
    std::string slot2 = rng.sample(symbols);
    std::string slot3 = rng.sample(symbols);
    
    // Check for wins
    uint32_t payout = 0;
    std::string result = slot1 + " " + slot2 + " " + slot3;
    
    if (slot1 == slot2 && slot2 == slot3) {
        if (slot1 == "💎") payout = 1000;      // Jackpot!
        else if (slot1 == "⭐") payout = 500;   // Stars
        else payout = 100;                      // Three of a kind
    } else if (slot1 == slot2 || slot2 == slot3 || slot1 == slot3) {
        payout = 10;  // Pair
    }
    
    // Record the result
    GameResult game{0, player_name, "slots", result, payout, ctx.timestamp};
    ctx.db[game_results].insert(game);
    return Ok();
}
```

### Technical Details

- **Algorithm**: Uses C++20's `std::mt19937_64` (64-bit Mersenne Twister)
- **Seeding**: Timestamp microseconds since Unix epoch
- **Thread Safety**: Each reducer call gets its own RNG instance
- **Performance**: Lazy initialization - RNG is only created when first accessed
- **Memory**: Uses `std::shared_ptr` to maintain copyability of `ReducerContext`

## Types and Serialization

### Primitive Types

All standard C++ numeric types are supported:

```cpp
struct AllTypes {
    // Integers
    uint8_t u8_field;
    uint16_t u16_field;
    uint32_t u32_field;
    uint64_t u64_field;
    int8_t i8_field;
    int16_t i16_field;
    int32_t i32_field;
    int64_t i64_field;
    
    // Large integers (SpacetimeDB types)
    SpacetimeDb::u128 u128_field;
    SpacetimeDb::u256 u256_field;
    SpacetimeDb::i128 i128_field;
    SpacetimeDb::i256 i256_field;
    
    // Floating point
    float f32_field;
    double f64_field;
    
    // Other
    bool bool_field;
    std::string string_field;
};
SPACETIMEDB_STRUCT(AllTypes, u8_field, u16_field, /* ... all fields ... */);
```

### Container Types

```cpp
struct WithContainers {
    std::vector<uint32_t> numbers;
    std::vector<std::string> names;
    std::optional<std::string> description;
    std::optional<std::vector<uint32_t>> optional_numbers;
};
SPACETIMEDB_STRUCT(WithContainers, numbers, names, description, optional_numbers);
```

### Custom Enums

#### Simple Enums (Unit Variants)

```cpp
// Using SPACETIMEDB_ENUM macro (recommended)
SPACETIMEDB_ENUM(Status, Pending, Active, Inactive)

// Manual implementation for complex cases
enum class Priority : uint8_t { Low = 0, Medium = 1, High = 2 };

namespace SpacetimeDb::bsatn {
template<>
struct bsatn_traits<Priority> {
    static AlgebraicType algebraic_type() {
        return LazyTypeRegistrar<Priority>::getOrRegister([]() {
            SumTypeBuilder builder;
            builder.with_unit_variant("Low");
            builder.with_unit_variant("Medium");
            builder.with_unit_variant("High");
            return AlgebraicType::make_sum(builder.build());
        }, "Priority");
    }
    
    static void serialize(Writer& writer, const Priority& value) {
        writer.write_u8(static_cast<uint8_t>(value));
    }
    
    static Priority deserialize(Reader& reader) {
        return static_cast<Priority>(reader.read_u8());
    }
};
}
```

#### Variant Enums (With Payloads)

```cpp
struct ErrorInfo { std::string message; };
SPACETIMEDB_STRUCT(ErrorInfo, message);

// Enum with different payload types
SPACETIMEDB_ENUM(Result,
    (Success, uint32_t),
    (Error, ErrorInfo),
    (Pending, std::monostate)  // Unit variant
)
```

### Namespace Qualification for Enums

Add namespace qualification to enums for better organization in generated client code:

```cpp
// Define the enum normally
SPACETIMEDB_ENUM(UserRole, Admin, Moderator, Member)

// Add namespace qualification (separate macro)
SPACETIMEDB_NAMESPACE(UserRole, "Auth")  // Will be "Auth.UserRole" in client code

// Multiple enums can share the same namespace
SPACETIMEDB_ENUM(Permission, Read, Write, Execute, Delete)
SPACETIMEDB_NAMESPACE(Permission, "Auth")  // Will be "Auth.Permission"

// Works with variant enums too
SPACETIMEDB_ENUM(NetworkEvent,
    (Connected, ConnectionInfo),
    (Disconnected, std::string),
    (Error, ErrorDetails)
)
SPACETIMEDB_NAMESPACE(NetworkEvent, "Network")  // Will be "Network.NetworkEvent"
```

**How It Works**:
- The `SPACETIMEDB_NAMESPACE` macro adds compile-time metadata to qualify the type name
- Client code generators recognize the namespace and organize types accordingly
- The namespace prefix appears in generated TypeScript, C#, and Rust client code
- Server-side C++ code continues to use the unqualified name

**Benefits**:
- Better organization of related types in client code
- Avoids naming conflicts in large projects
- Clearer API structure for client developers
- No runtime overhead - purely compile-time feature

### Custom Structs

```cpp
struct Address {
    std::string street;
    std::string city;
    std::string country;
};
SPACETIMEDB_STRUCT(Address, street, city, country);

struct Person {
    uint32_t id;
    std::string name;
    Address address;  // Nested struct
    std::vector<std::string> hobbies;
};
SPACETIMEDB_STRUCT(Person, id, name, address, hobbies);
```

## Constraints and Indexing

### Constraint Types

Constraints are applied **after** table registration using `FIELD_` macros:

```cpp
SPACETIMEDB_TABLE(User, users, Public);

// Primary key (unique + clustered btree index)
FIELD_PrimaryKey(users, id);

// Auto-incrementing primary key  
FIELD_PrimaryKeyAutoInc(users, id);

// Unique constraint (creates btree index)
FIELD_Unique(users, email);

// Auto-incrementing unique field
FIELD_UniqueAutoInc(users, sequence_num);

// Btree index for fast queries and range operations
FIELD_Index(users, age);

// Auto-incrementing indexed field
FIELD_IndexAutoInc(users, order_id);

// Multi-column btree index
FIELD_NamedMultiColumnIndex(users, age_city_idx, age, city);
```

### Constraint Requirements

| Constraint Type | Allowed Types |
|-----------------|---------------|
| PrimaryKey | Integers, bool, string, Identity, ConnectionId, Timestamp, enums |
| Unique | Same as PrimaryKey |
| Index | Same as PrimaryKey |
| AutoInc | Integer types only |

### Auto-Increment Callbacks

When using auto-increment fields, the `insert()` method automatically returns the row with the generated ID populated. This enables immediate access to generated values without requiring additional lookups.

```cpp
// Table with auto-increment ID
SPACETIMEDB_TABLE(User, users, Public);
FIELD_PrimaryKeyAutoInc(users, id);

SPACETIMEDB_REDUCER(create_user, ReducerContext ctx, std::string name) {
    User user{0, name, true};  // id=0 is placeholder - will be auto-generated
    
    // insert() returns the user with the generated ID
    User created_user = ctx.db[users].insert(user);
    
    // Generated ID is immediately available
    LOG_INFO("Created user " + name + " with ID: " + std::to_string(created_user.id));
    
    // Can use the ID for related operations
    if (created_user.id > 1000) {
        LOG_INFO("High-value user created");
    }
}

// Works with all auto-increment constraint types
FIELD_UniqueAutoInc(orders, order_number);
FIELD_IndexAutoInc(events, sequence);
FIELD_AutoInc(logs, entry_id);
```

**How It Works**:
1. SpacetimeDB generates the auto-increment value server-side
2. Only the generated column values are returned (not the full row)
3. The SDK automatically integrates the generated values back into your row object
4. The `insert()` method returns the complete row with generated fields populated

**Multiple Auto-Increment Fields**:
If a table has multiple auto-increment fields, all generated values are integrated:

```cpp
struct LogEntry {
    uint32_t id;          // Auto-increment primary key
    uint64_t sequence;    // Auto-increment sequence number
    std::string message;
};

SPACETIMEDB_TABLE(LogEntry, logs, Private);
FIELD_PrimaryKeyAutoInc(logs, id);
FIELD_UniqueAutoInc(logs, sequence);

SPACETIMEDB_REDUCER(log_message, ReducerContext ctx, std::string msg) {
    LogEntry entry{0, 0, msg};  // Both id and sequence will be generated
    LogEntry created = ctx.db[logs].insert(entry);
    
    LOG_INFO("Log entry " + std::to_string(created.id) + 
             " with sequence " + std::to_string(created.sequence));
}
```

### Using Indexed Fields

```cpp
// Primary key access
SPACETIMEDB_REDUCER(get_user, ReducerContext ctx, uint32_t user_id) {
    // Efficient O(log n) lookup
    auto user_opt = ctx.db[users_id].find(user_id);
    if (user_opt.has_value()) {
        LOG_INFO("Found user: " + user_opt->name);
    }
    return Ok();
}

// Unique field access
SPACETIMEDB_REDUCER(find_by_email, ReducerContext ctx, std::string email) {
    for (const auto& user : ctx.db[users_email].filter(email)) {
        LOG_INFO("User with email " + email + ": " + user.name);
        break; // Unique, so only one result
    }
    return Ok();
}

// Non-unique index
SPACETIMEDB_REDUCER(users_by_age, ReducerContext ctx, uint32_t age) {
    for (const auto& user : ctx.db[users_age].filter(age)) {
        LOG_INFO("User age " + std::to_string(age) + ": " + user.name);
    }
    return Ok();
}
```

### Range Queries

Btree indexes support efficient range queries using the C++ range query system:

```cpp
#include <spacetimedb/range_queries.h>

SPACETIMEDB_TABLE(Product, products, Public);
FIELD_Index(products, price);
FIELD_Index(products, category);

SPACETIMEDB_REDUCER(products_in_price_range, ReducerContext ctx, double min_price, double max_price) {
    // Create range objects
    auto price_range = range_inclusive(min_price, max_price);  // min_price..=max_price
    auto expensive_items = range_from(100.0);                 // >= 100.0
    auto cheap_items = range_to(50.0);                        // < 50.0
    
    // Use indexed field accessor for efficient queries
    for (const auto& product : ctx.db[products_price].filter(price_range)) {
        LOG_INFO("Product in range: " + product.name + " - $" + std::to_string(product.price));
    }
    return Ok();
}

// All range construction patterns
SPACETIMEDB_REDUCER(demonstrate_ranges, ReducerContext ctx) {
    auto range1 = range_from(25);                    // 25..  (>= 25)
    auto range2 = range_to(30);                      // ..30  (< 30)
    auto range3 = range(20, 35);                     // 20..35 (>= 20, < 35)
    auto range4 = range_inclusive(20, 35);           // 20..=35 (>= 20, <= 35)
    auto range5 = range_to_inclusive(30);            // ..=30 (>= 30)
    auto range6 = range_full<int>();                 // .. (all values)
    
    // Check if value is in range
    bool in_range = range4.contains(25);  // true
    
    // Range queries work with any indexed type
    auto name_range = range(std::string("A"), std::string("M")); // Names A-L
    for (const auto& user : ctx.db[users_name].filter(name_range)) {
        LOG_INFO("User: " + user.name);
    }
    return Ok();
}
```

## Client Visibility Filters

Control what data clients can see using row-level security:

```cpp
// Only show users their own data
SPACETIMEDB_CLIENT_VISIBILITY_FILTER(user_data_filter, 
    "SELECT * FROM user_data WHERE owner_identity = current_user_identity()"
);

// Show public posts and user's own private posts
SPACETIMEDB_CLIENT_VISIBILITY_FILTER(posts_filter,
    "SELECT * FROM posts WHERE is_public = true OR author_identity = current_user_identity()"
);

// Time-based visibility (only show recent messages)
SPACETIMEDB_CLIENT_VISIBILITY_FILTER(recent_messages,
    "SELECT * FROM messages WHERE timestamp > (current_timestamp() - INTERVAL '1 day')"
);

struct UserData {
    uint32_t id;
    Identity owner_identity;
    std::string private_info;
    bool is_sensitive;
};
SPACETIMEDB_STRUCT(UserData, id, owner_identity, private_info, is_sensitive);
SPACETIMEDB_TABLE(UserData, user_data, Public);  // Public table with RLS

struct Post {
    uint32_t id;
    Identity author_identity;
    std::string content;
    bool is_public;
    Timestamp created_at;
};
SPACETIMEDB_STRUCT(Post, id, author_identity, content, is_public, created_at);
SPACETIMEDB_TABLE(Post, posts, Public);  // Filtered by posts_filter
```

**Available SQL functions for filters:**
- `current_user_identity()` - Get the calling client's Identity
- `current_timestamp()` - Get current server timestamp
- Standard SQL operators and functions
- Table and column references

## Special Types

SpacetimeDB provides built-in types for common use cases:

### Identity

Represents a unique user/client identity:

```cpp
struct User {
    Identity id;        // Unique across all users
    std::string name;
};

SPACETIMEDB_REDUCER(create_user, ReducerContext ctx, std::string name) {
    User user{ctx.sender, name};  // ctx.sender is the calling client's identity
    ctx.db[users].insert(user);
    return Ok();
}
```

### ConnectionId

Represents a specific client connection:

```cpp
struct Session {
    ConnectionId connection;
    Identity user;
    Timestamp login_time;
};
```

### Timestamp

Represents a point in time:

```cpp
struct Event {
    std::string name;
    Timestamp when;
};

SPACETIMEDB_REDUCER(log_event, ReducerContext ctx, std::string event_name) {
    Event event{event_name, ctx.timestamp};  // Current time
    ctx.db[events].insert(event);
    return Ok();
}
```

### TimeDuration

Represents a duration of time:

```cpp
struct Task {
    std::string name;
    TimeDuration estimated_duration;
};

// Create durations
auto one_hour = TimeDuration::from_hours(1);
auto five_minutes = TimeDuration::from_millis(5 * 60 * 1000);
```

### Scheduled Reducers

Schedule reducers to run automatically at specified times:

```cpp
// Define a table to store scheduled tasks
struct ScheduledTask {
    uint32_t id;
    ScheduleAt run_at;         // When to execute
    std::string task_data;
    Timestamp created_at;
};
SPACETIMEDB_STRUCT(ScheduledTask, id, run_at, task_data, created_at);
SPACETIMEDB_TABLE(ScheduledTask, scheduled_tasks, Private);
FIELD_PrimaryKeyAutoInc(scheduled_tasks, id);

// Register the table for scheduling (column 1 = run_at field, index 0-based)
SPACETIMEDB_SCHEDULE(scheduled_tasks, 1, process_scheduled_task);

// The scheduled reducer - called automatically when tasks are due
SPACETIMEDB_REDUCER(process_scheduled_task, ReducerContext ctx, ScheduledTask task) {
    LOG_INFO("Processing scheduled task: " + task.task_data);
    
    // Process the task...
    
    // Optionally schedule another task
    auto next_run = ScheduleAt(TimeDuration::from_hours(24)); // Run in 24 hours
    ScheduledTask next_task{0, next_run, "Daily cleanup", ctx.timestamp};
    ctx.db[scheduled_tasks].insert(next_task);
    return Ok();
}

// Create scheduled tasks from other reducers
SPACETIMEDB_REDUCER(schedule_reminder, ReducerContext ctx, std::string message, uint64_t delay_seconds) {
    auto run_time = ScheduleAt(TimeDuration::from_secs(delay_seconds));
    ScheduledTask reminder{0, run_time, message, ctx.timestamp};
    ctx.db[scheduled_tasks].insert(reminder);
    
    LOG_INFO("Reminder scheduled for " + std::to_string(delay_seconds) + " seconds");
    return Ok();
}

// ScheduleAt can be created with TimeDuration (relative) or Timestamp (absolute)
SPACETIMEDB_REDUCER(schedule_examples, ReducerContext ctx) {
    // Relative scheduling (from now)
    auto in_one_hour = ScheduleAt(TimeDuration::from_hours(1));
    auto in_five_minutes = ScheduleAt(TimeDuration::from_millis(5 * 60 * 1000));
    
    // Absolute scheduling (specific time)
    auto specific_time = ScheduleAt(Timestamp::from_millis(1640995200000)); // Specific Unix timestamp
    
    // Schedule tasks
    ctx.db[scheduled_tasks].insert(ScheduledTask{0, in_one_hour, "Hourly task", ctx.timestamp});
    ctx.db[scheduled_tasks].insert(ScheduledTask{0, in_five_minutes, "Quick task", ctx.timestamp});
    return Ok();
}
```

**Key points about scheduled reducers:**
- Must have a table with a `ScheduleAt` field
- Use `SPACETIMEDB_SCHEDULE(table_name, column_index, reducer_name)` to register
- The scheduled reducer receives the entire row as parameter
- Column index is 0-based (0 = first field, 1 = second field, etc.)
- Scheduled reducers run with module identity, not client identity

## Logging

SpacetimeDB provides structured logging:

```cpp
SPACETIMEDB_REDUCER(example, ReducerContext ctx) {
    LOG_DEBUG("Debug information");
    LOG_INFO("General information");
    LOG_WARN("Warning message");
    LOG_ERROR("Error occurred");
    LOG_PANIC("Fatal error");  // Terminates reducer
    
    // With timing
    {
        LogStopwatch timer("Database operation");
        // ... time-consuming operation ...
    } // Automatically logs duration
    return Ok();
}
```

## Build System

### CMake Configuration

The C++ SDK uses CMake with Emscripten for WebAssembly compilation:

```cmake
# Basic configuration
cmake_minimum_required(VERSION 3.16)
project(my-module)
set(CMAKE_CXX_STANDARD 20)

# Module source (defaults to src/lib.cpp)
if(NOT DEFINED MODULE_SOURCE)
    set(MODULE_SOURCE "src/lib.cpp")
endif()

# Output name (defaults to "lib")
if(NOT DEFINED OUTPUT_NAME)
    set(OUTPUT_NAME "lib")
endif()

# Link SpacetimeDB library
set(SPACETIMEDB_CPP_LIBRARY_PATH "path/to/bindings-cpp")
add_executable(${OUTPUT_NAME} ${MODULE_SOURCE})
target_include_directories(${OUTPUT_NAME} PRIVATE ${SPACETIMEDB_CPP_LIBRARY_PATH}/include)
add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} spacetimedb_cpp_library)
target_link_libraries(${OUTPUT_NAME} PRIVATE spacetimedb_cpp_library)
```

### Build Commands

```bash
# Standard build
emcmake cmake -B build .
cmake --build build

# Custom module source
emcmake cmake -B build -DMODULE_SOURCE=src/test.cpp -DOUTPUT_NAME=test .
cmake --build build
# Creates build/test.wasm

# Publishing
spacetime publish --bin-path build/lib.wasm my-database
# Or auto-detect
spacetime publish . my-database
```

### Emscripten Settings

The build system automatically configures:
- WebAssembly output format
- Required exports for SpacetimeDB
- Memory settings optimized for database operations
- Exception handling disabled for WASM compatibility

## Examples

### Complete User Management System

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDb;

// User data structure
struct User {
    uint32_t id;
    Identity identity;
    std::string username;
    std::string email;
    Timestamp created_at;
    bool active;
};
SPACETIMEDB_STRUCT(User, id, identity, username, email, created_at, active);

// User table with constraints
SPACETIMEDB_TABLE(User, users, Public);
FIELD_PrimaryKeyAutoInc(users, id);
FIELD_Unique(users, identity);
FIELD_Unique(users, username);
FIELD_Unique(users, email);
FIELD_Index(users, active);

// Register new user
SPACETIMEDB_REDUCER(register_user, ReducerContext ctx, std::string username, std::string email) {
    // Check if user already exists
    for (const auto& user : ctx.db[users_identity].filter(ctx.sender)) {
        if (user.active) {
            return Err("User already registered");
        }
    }
    
    User new_user{0, ctx.sender, username, email, ctx.timestamp, true};
    ctx.db[users].insert(new_user);
    LOG_INFO("User registered: " + username);
    return Ok();
}

// Update user profile
SPACETIMEDB_REDUCER(update_profile, ReducerContext ctx, std::string new_username) {
    for (auto& user : ctx.db[users]) {
        if (user.identity == ctx.sender && user.active) {
            user.username = new_username;
            ctx.db[users].update(user);
            LOG_INFO("Profile updated");
            return Ok();
        }
    }
    return Err("User not found or inactive");
}

// Deactivate user
SPACETIMEDB_REDUCER(deactivate_user, ReducerContext ctx) {
    for (auto& user : ctx.db[users]) {
        if (user.identity == ctx.sender) {
            user.active = false;
            ctx.db[users].update(user);
            LOG_INFO("User deactivated");
            return Ok();
        }
    }
    return Ok();
}

// Admin: List all active users
SPACETIMEDB_REDUCER(list_active_users, ReducerContext ctx) {
    for (const auto& user : ctx.db[users_active].filter(true)) {
        LOG_INFO("Active user: " + user.username + " (" + user.email + ")");
    }
    return Ok();
}

// Lifecycle: Track connections
SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) {
    LOG_INFO("Client connected");
    return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) {
    LOG_INFO("Client disconnected");
    return Ok();
}
```

### Advanced: Chat System with Channels

```cpp
// Channel structure
struct Channel {
    uint32_t id;
    std::string name;
    std::string description;
    Identity owner;
    bool public_channel;
};
SPACETIMEDB_STRUCT(Channel, id, name, description, owner, public_channel);
SPACETIMEDB_TABLE(Channel, channels, Public);
FIELD_PrimaryKeyAutoInc(channels, id);
FIELD_Unique(channels, name);

// Message structure
struct Message {
    uint32_t id;
    uint32_t channel_id;
    Identity sender;
    std::string content;
    Timestamp timestamp;
};
SPACETIMEDB_STRUCT(Message, id, channel_id, sender, content, timestamp);
SPACETIMEDB_TABLE(Message, messages, Public);
FIELD_PrimaryKeyAutoInc(messages, id);
FIELD_Index(messages, channel_id);
FIELD_Index(messages, sender);

// Create channel
SPACETIMEDB_REDUCER(create_channel, ReducerContext ctx, std::string name, std::string description, bool is_public) {
    Channel channel{0, name, description, ctx.sender, is_public};
    ctx.db[channels].insert(channel);
    LOG_INFO("Channel created: " + name);
    return Ok();
}

// Send message to channel
SPACETIMEDB_REDUCER(send_message, ReducerContext ctx, uint32_t channel_id, std::string content) {
    // Verify channel exists
    bool channel_exists = false;
    for (const auto& channel : ctx.db[channels]) {
        if (channel.id == channel_id) {
            channel_exists = true;
            break;
        }
    }
    
    if (!channel_exists) {
        LOG_ERROR("Channel not found");
        return Err("Channel not found");
    }
    
    Message message{0, channel_id, ctx.sender, content, ctx.timestamp};
    ctx.db[messages].insert(message);
    return Ok();
}

// Get channel history
SPACETIMEDB_REDUCER(get_channel_history, ReducerContext ctx, uint32_t channel_id) {
    for (const auto& message : ctx.db[messages_channel_id].filter(channel_id)) {
        LOG_INFO("Message: " + message.content);
    }
    return Ok();
}
```

## Error Handling

### Constraint Violations

```cpp
SPACETIMEDB_REDUCER(create_user, ReducerContext ctx, std::string username) {
    User user{0, ctx.sender, username, ctx.timestamp};
    
    // If username is not unique, this will fail the entire transaction
    ctx.db[users].insert(user);
    
    // This line won't execute if the insert fails
    LOG_INFO("User created successfully");
    return Ok();
}
```

### Validation Patterns

```cpp
SPACETIMEDB_REDUCER(update_age, ReducerContext ctx, uint32_t new_age) {
    if (new_age > 150) {
        LOG_ERROR("Invalid age: " + std::to_string(new_age));
        return Err("Invalid age value");
    }
    
    // Find and update user...
    return Ok();
}
```

---

This completes the C++ reference documentation. For more examples and advanced patterns, see the working modules in [`modules/sdk-test-cpp`](../../modules/sdk-test-cpp) and [`modules/module-test-cpp`](../../modules/module-test-cpp).