# SpacetimeDB C++ Module Quickstart

This guide will walk you through creating your first SpacetimeDB module in C++. We'll build a simple chat server to demonstrate the core concepts.

## What is a SpacetimeDB Module?

A SpacetimeDB module is C++ code that gets compiled to WebAssembly and runs inside the database. Instead of the traditional architecture (database → app server → clients), SpacetimeDB lets you write your entire backend logic that runs **inside** the database itself, giving you microsecond latency and automatic real-time sync to clients.

Modules consist of four main components:

- **Tables**: Database tables defined as C++ structs
- **Reducers**: Functions that modify data and can be called by clients
- **Views**: Read-only query functions that return data (std::vector<T> or std::optional<T>) to clients
- **Procedures**: Pure functions that return values and can optionally access the database via transactions 

## Prerequisites

Before we begin, make sure you have:

- [SpacetimeDB CLI](https://spacetimedb.com/install) installed
- [Emscripten SDK (emsdk)](https://emscripten.org/docs/getting_started/downloads.html) for WebAssembly compilation
- CMake 3.16+ 
- A C++20 compatible compiler

## Creating Your First Module

### Step 1: Initialize the Project

Create a new C++ module using the SpacetimeDB CLI:

```bash
spacetime init --lang cpp my-chat-module
cd my-chat-module
```

This creates a project with the following structure:
```
my-chat-module/spacetimedb/
├── CMakeLists.txt
├── src/
    └── lib.cpp
└── .gitignore
```

### Step 2: Define Your Data Structures

Open `lib.cpp` and replace the generated code with our chat server implementation:

```cpp
#include <spacetimedb.h>

using namespace SpacetimeDB;

// Define a User table to store connected users
struct User {
    Identity identity;      // SpacetimeDB's built-in user identity type
    std::optional<std::string> name;  // User's display name (optional)
    bool online;           // Whether the user is currently connected
};

// Register the struct for BSATN serialization
SPACETIMEDB_STRUCT(User, identity, name, online)

// Register as a public table with identity as primary key
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKey(user, identity);

// Define a Message table to store chat messages
struct Message {
    Identity sender;       // Who sent the message
    Timestamp sent;        // When the message was sent
    std::string text;      // Message content
};

SPACETIMEDB_STRUCT(Message, sender, sent, text)
SPACETIMEDB_TABLE(Message, message, Public)
```

### Step 3: Add Helper Functions and Reducers

First, add validation helper functions:

```cpp
// Validate that a name is not empty, return an Outcome which houses a error as std::string
Outcome<std::string> validate_name(const std::string& name) {
    if (name.empty()) {
        return Err<std::string>("Names must not be empty");
    }
    return Ok(name);
}

// Validate that a message is not empty, return an Outcome which houses a error as std::string
Outcome<std::string> validate_message(const std::string& text) {
    if (text.empty()) {
        return Err<std::string>("Messages must not be empty");
    }
    return Ok(text);
}
```

Now add the reducers (functions that clients can call to modify the database):

```cpp
// Called when a user sets their name
SPACETIMEDB_REDUCER(set_name, ReducerContext ctx, std::string name) {
    auto validated = validate_name(name);
    if (validated.is_err()) {
        return Err(validated.error());
    }
    
    // Find and update the user by identity (primary key)
    auto user_row = ctx.db[user_identity].find(ctx.sender);
    if (user_row.has_value()) {
        auto user = user_row.value();
        user.name = validated.value();
        ctx.db[user_identity].update(user);
        return Ok();
    }
    
    return Err("Cannot set name for unknown user");
}

// Called when a user sends a message
SPACETIMEDB_REDUCER(send_message, ReducerContext ctx, std::string text) {
    auto validated = validate_message(text);
    if (validated.is_err()) {
        return Err(validated.error());
    }
    
    Message msg{ctx.sender, ctx.timestamp, validated.value()};
    ctx.db[message].insert(msg);
    return Ok();
}
```

### Step 4: Add Lifecycle Reducers

Lifecycle reducers are special functions called automatically by SpacetimeDB:

```cpp
// Called when a client connects
SPACETIMEDB_CLIENT_CONNECTED(client_connected, ReducerContext ctx) {
    auto user_row = ctx.db[user_identity].find(ctx.sender);
    if (user_row.has_value()) {
        auto user = user_row.value();
        user.online = true;
        ctx.db[user_identity].update(user);
    } else {
        User new_user{ctx.sender, std::nullopt, true};
        ctx.db[user].insert(new_user);
    }
    return Ok();
}

// Called when a client disconnects  
SPACETIMEDB_CLIENT_DISCONNECTED(client_disconnected, ReducerContext ctx) {
    auto user_row = ctx.db[user_identity].find(ctx.sender);
    if (user_row.has_value()) {
        auto user = user_row.value();
        user.online = false;
        ctx.db[user_identity].update(user);
    } else {
        LOG_WARN("Disconnect event for unknown user");
    }
    return Ok();
}
```

### Step 5: Build Your Module

Build the module using the provided CMake configuration:

```bash
spacetime build -p ./spacetimedb
```

This compiles your C++ code to WebAssembly, producing `build/lib.wasm`.

### Step 6: Publish to SpacetimeDB

Start your local SpacetimeDB instance:

```bash
spacetime start
```

Publish your module:

```bash
spacetime publish . my-chat-db
```

### Step 7: Test Your Module

You can test your reducers using the CLI:

```bash
# Set a user's name
spacetime call my-chat-db set_name "Alice"

# Send a message
spacetime call my-chat-db send_message "Hello, world!"

# View all users
spacetime sql my-chat-db "SELECT * FROM user"

# View all messages
spacetime sql my-chat-db "SELECT * FROM message"
```

## Key Concepts Explained

### Tables vs. Structs

- **Structs** are just data types - they need `SPACETIMEDB_STRUCT` for serialization
- **Tables** are database tables created with `SPACETIMEDB_TABLE` and store data persistently
- The same struct can be used for multiple tables or just as a data type

### Database Access Pattern

SpacetimeDB C++ uses a unique accessor pattern:
- `ctx.db[tableName]` - Access table for iteration and basic operations, eg. ctx.db[user]
- `ctx.db[tableName_fieldName]` - Access indexed fields for optimized operations, eg. ctx.db[user_id]

```cpp
// Table access
ctx.db[user].insert(new_user);

// Field accessor (for indexed fields, e.g., user_identity = table 'user' + field 'identity')
ctx.db[user_identity].delete_by_key(identity);
```

### Constraints and Indexes

Constraints are applied **after** table registration using `FIELD_` macros:

```cpp
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKey(user, identity);       // Primary key
FIELD_Unique(user, email);              // Unique constraint
FIELD_Index(user, age);                 // Index for fast queries
```

### Public vs. Private Tables

- **Public tables**: Automatically synced to subscribed clients
- **Private tables**: Only accessible by reducers, not synced to clients

## Next Steps

Now that you have a basic chat server:

1. **Add more features**: User roles, message editing, channels
2. **Add constraints**: Unique usernames, message length limits
3. **Explore indexing**: For fast queries on large datasets
4. **Try scheduled reducers**: For periodic cleanup or notifications
5. **Generate client code**: Use `spacetime generate` to create TypeScript, C#, or Rust clients

## Advanced Example: Adding Indexes

You can add an index to make querying messages by sender more efficient:

```cpp
SPACETIMEDB_TABLE(Message, message, Public)
FIELD_Index(message, sender);  // Add index on sender for faster queries

// Now you can efficiently query by sender using the field accessor:
SPACETIMEDB_REDUCER(get_user_messages, ReducerContext ctx, Identity user_identity) {
    for (const auto& msg : ctx.db[message_sender].filter(user_identity)) {
        LOG_INFO("Message: " + msg.text);
    }
    return Ok();
}
```

## Troubleshooting

**Build errors**: Ensure you have the latest Emscripten SDK and are using `emcmake cmake`

**Module not found**: Check that SpacetimeDB is running

**Type errors**: Remember that C++ types need exact matches - use `uint32_t`, not `int`

**Constraint violations**: Constraints are enforced by the database - duplicate primary keys will cause reducers to fail

## Example Projects

For more complex examples, see:
- [`modules/sdk-test-cpp/src/lib.cpp`](../../modules/sdk-test-cpp/src/lib.cpp) - Comprehensive type and operation testing
- [`modules/module-test-cpp/src/lib.cpp`](../../modules/module-test-cpp/src/lib.cpp) - Advanced indexing and enum examples

---

Ready to learn more? Check out the [C++ Reference Documentation](REFERENCE.md) for detailed API information.