# SpacetimeDB C++ Module Library

The SpacetimeDB C++ Module Library provides a modern C++20 API for building SpacetimeDB modules that run inside the database as WebAssembly.

## Current State

This library provides a production-ready C++ SDK for SpacetimeDB with complete type system support:

### ✅ Features
- Module compilation and publishing to SpacetimeDB
- All lifecycle reducers (init, client_connected, client_disconnected)
- User-defined reducers with unlimited parameters
- Table registration with constraints (PrimaryKey, Unique, AutoInc)
- Insert and delete operations
- All primitive types (u8-u256, i8-i256, bool, f32, f64, string)
- All special types (Identity, ConnectionId, Timestamp, TimeDuration)
- Vector types for all primitives and special types
- Optional types (std::optional<T>)
- Custom struct serialization via BSATN
- Complex enum support with proper variant names
- Enhanced logging system with file/line info
- Mixed type combinations - handle any complexity level

### 🏗️ Architecture
- **Hybrid Compile-Time/Runtime System**: C++20 concepts for compile-time validation with __preinit__ runtime registration
- **V9 Type Registration System**: Unified type registration with comprehensive error detection and circular reference prevention
- **Nominal Type System**: Types identified by their declared names with explicit registration via SPACETIMEDB_STRUCT macros
- **Multi-Layer Validation**: Static assertions, runtime constraint checking, and error module replacement strategy

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed technical documentation.

### ✅ Advanced Features Available
- **Btree indexes**: Full support with `FIELD_Index` macros and optimized queries
- **Range queries**: Complete range query system with `range_from()`, `range_to()`, `range_inclusive()`, etc.
- **Client visibility filters**: Row-level security with `SPACETIMEDB_CLIENT_VISIBILITY_FILTER` macro
- **Scheduled reducers**: `SPACETIMEDB_SCHEDULE` macro for time-based execution
- **Field accessor patterns**: Efficient indexed operations with `ctx.db[table_field]`

### ❌ Not Yet Implemented  
- Direct SQL query execution within modules (SQL available via CLI: `spacetime sql`)
- Automatic migrations (limited - only adding tables supported)
- Complex table schema changes
- Transactions (individual reducers are transactional)

See the working examples in `modules/sdk-test-cpp/src/lib.cpp` for comprehensive feature usage.

## Features

- **Modern C++20 API**: Uses concepts, structured bindings, and other C++20 features
- **BSATN Serialization**: Binary Serialization And Type Notation for efficient data transfer
- **Automatic Field Registration**: Tables register their fields using SPACETIMEDB_STRUCT macro
- **Unified Reducer System**: Single macro for all reducer types with automatic lifecycle detection
- **Type-Safe Database Access**: Template-based table accessors with compile-time type checking
- **Memory Safety**: WASI shims for safe memory operations in WebAssembly environment
- **Enhanced Logging**: Multiple log levels with file/line information
- **Namespace Support**: Clean namespace qualification for enums with just 2 lines of code

## Quick Start

### Option 1: Using spacetime init (Recommended)

```bash
# Create a new C++ project
spacetime init --lang cpp my-project
cd my-project

# Build and publish
emcmake cmake -B build .
cmake --build build
spacetime publish . my-database
```

### Option 2: Manual Setup

For existing projects, add the following to your C++ module:

```cpp
#include <spacetimedb.h>

using namespace SpacetimeDb;

// Define a table structure
struct User {
    uint32_t id;
    std::string name;
    std::string email;
};

// Register BSATN serialization
SPACETIMEDB_STRUCT(User, id, name, email)

// Register as a table
SPACETIMEDB_TABLE(User, users, Public)

// Add constraints using FIELD_ macros
FIELD_PrimaryKeyAutoInc(users, id);
FIELD_Unique(users, email);

// Define an enum with namespace qualification
SPACETIMEDB_ENUM(UserRole, Admin, Moderator, Member)
SPACETIMEDB_NAMESPACE(UserRole, "Auth")  // Will be "Auth.UserRole" in client code

// User-defined reducer
SPACETIMEDB_REDUCER(add_user, ReducerContext ctx, std::string name, std::string email) {
    User user{0, name, email}; // id will be auto-generated
    ctx.db[users].insert(user);
    LOG_INFO("Added user: " + name);
}

// Delete user by id (using primary key)
SPACETIMEDB_REDUCER(delete_user, ReducerContext ctx, uint32_t id) {
    ctx.db[users_id].delete_by_key(id);
}
```

**Note:** Lifecycle reducers (`SPACETIMEDB_INIT`, `SPACETIMEDB_CLIENT_CONNECTED`, `SPACETIMEDB_CLIENT_DISCONNECTED`) are available but not shown in working examples. See `reducer_macros.h` for details.

## Building Modules

### Prerequisites
- Emscripten SDK (emsdk)
- CMake 3.16+
- C++20 compatible compiler

### Build Steps

```bash
# Navigate to your module directory
cd modules/your-module

# Configure with CMake (uses src/lib.cpp by default)
emcmake cmake -B build .

# Build the module
cmake --build build

# Publish to SpacetimeDB
spacetime publish --bin-path build/lib.wasm your-database-name
# Or use the directory (auto-detects build/lib.wasm)
spacetime publish . your-database-name
```

#### Custom Module Source

To build a different source file:

```bash
# Build a specific test module
emcmake cmake -B build -DMODULE_SOURCE=src/test_module.cpp -DOUTPUT_NAME=test_module .
cmake --build build
# This creates build/test_module.wasm
```

## API Reference

### Macros

#### Table Definition
- `SPACETIMEDB_TABLE(Type, table_name, Public/Private)` - Register a table
- `SPACETIMEDB_STRUCT(Type, field1, field2, ...)` - Register type for BSATN serialization

#### Enum Definition
- `SPACETIMEDB_ENUM(EnumName, Value1, Value2, ...)` - Define a simple enum
- `SPACETIMEDB_ENUM(EnumName, (Variant1, Type1), (Variant2, Type2), ...)` - Define an enum with payloads
- `SPACETIMEDB_NAMESPACE(EnumName, "Namespace")` - Add namespace qualification to an enum

#### Reducers
- `SPACETIMEDB_REDUCER(name, ReducerContext ctx, ...)` - User-defined reducer
- `SPACETIMEDB_INIT(name)` - Module initialization reducer
- `SPACETIMEDB_CLIENT_CONNECTED(name)` - Client connection reducer
- `SPACETIMEDB_CLIENT_DISCONNECTED(name)` - Client disconnection reducer

#### Field Constraints (applied after table registration)
- `FIELD_PrimaryKey(table_name, field)` - Primary key constraint
- `FIELD_PrimaryKeyAutoInc(table_name, field)` - Auto-incrementing primary key
- `FIELD_Unique(table_name, field)` - Unique constraint
- `FIELD_UniqueAutoInc(table_name, field)` - Auto-incrementing unique field
- `FIELD_Index(table_name, field)` - Index for faster queries
- `FIELD_IndexAutoInc(table_name, field)` - Auto-incrementing indexed field
- `FIELD_AutoInc(table_name, field)` - Auto-increment without other constraints

### Logging

```cpp
LOG_DEBUG("Debug message");
LOG_INFO("Info message");
LOG_WARN("Warning message");
LOG_ERROR("Error message");
LOG_PANIC("Fatal error message");

// With timing
{
    LogStopwatch timer("Operation name");
    // ... code to time ...
} // Automatically logs duration
```

## Architecture

The library uses a sophisticated hybrid compile-time/runtime architecture:

- **Compile-Time Validation** (`filterable_value_concept.h`, `table_with_constraints.h`): C++20 concepts and static assertions for constraint validation
- **V9 Type Registration System** (`internal/v9_type_registration.h`): Unified type registration with error detection and circular reference prevention
- **Priority-Ordered Initialization** (`internal/Module.cpp`): __preinit__ functions with numbered priorities ensure correct registration order
- **Error Detection System** (`internal/Module.cpp`): Multi-layer validation with error module replacement for clear diagnostics
- **BSATN Serialization** (`bsatn/`): Binary serialization system with algebraic type support for all data types
- **Database Interface** (`database.h`, `table_with_constraints.h`): Type-safe table access with optimized field accessors
- **Reducer System** (`reducer_macros.h`): Unified macro system for all reducer types with parameter type capture
- **Logging** (`logger.h`): Comprehensive logging with source location tracking

For detailed technical documentation, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Limitations

1. **Type System**
   - Very large type combinations may exceed WASM memory limits
   - Complex recursive type references require careful ordering

2. **Database Operations**
   - Index-based operations use field accessors: `ctx.db[table_field].delete_by_key(value)`
   - Table constraints are declared and enforced by server
   - Supports insert, delete, and update operations through field accessors

3. **Advanced Features**  
   - **Btree indexes**: `FIELD_Index` creates btree indexes for efficient range queries
   - **Range queries**: Full support for `range_from()`, `range_to()`, `range_inclusive()`, etc.
   - **Client visibility filters**: Row-level security via `SPACETIMEDB_CLIENT_VISIBILITY_FILTER`
   - **Limited migrations**: Only adding tables supported automatically
   - **SQL execution**: Available via CLI (`spacetime sql`) but not within modules

## Examples

See the `modules/sdk-test-cpp/src/` directory for example modules:
- `lib.cpp` - Comprehensive working module with all primitive types, tables, and reducers
- Full equivalence with Rust and C# SDK test modules
- Examples of all constraint types and database operations

## Contributing

This library is part of the SpacetimeDB project. Please see the main repository for contribution guidelines.

## License

Apache License 2.0 - See LICENSE file in the root directory.