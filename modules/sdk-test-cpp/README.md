# `sdk-test-cpp` C++ test module

Exercise the functionality of the SpacetimeDB C++ bindings API surface, modeling all combinations
of types, with several examples of tables, indexes, and reducers.

Used to validate C++ bindings functionality and ensure parity with Rust/C# implementations.

> **Note:** Mirrors functionality from [`modules/sdk-test`](../sdk-test/) and [`modules/sdk-test-cs`](../sdk-test-cs/).

## Building

```bash
cd modules/sdk-test-cpp
emcmake cmake -B build .
cmake --build build
```

The built WASM module will be at `build/lib.wasm`.

## Testing

```bash
# Start SpacetimeDB
spacetime start

# Publish the module
spacetime publish . test-db --delete-data

# Verify module schema
spacetime describe test-db

# Call example reducer
spacetime call test-db add_player '"Alice"'

# View logs
spacetime logs test-db -f
```

## Module Contents

`lib.cpp` contains comprehensive testing of:
- All primitive types (integers, floats, bool, string)
- Special types (Identity, ConnectionId, Timestamp, TimeDuration)  
- Collections (vectors, optionals)
- Custom structs and enums
- Table constraints (PrimaryKey, Unique, AutoInc, indexes)
- Lifecycle reducers (init, connect, disconnect)
- CRUD operations

For C++ bindings usage documentation, see [`crates/bindings-cpp/`](../../crates/bindings-cpp/).