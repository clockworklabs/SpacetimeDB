# SDK Test Procedure C++ - Rust Client

Rust client for testing the sdk-test-procedure-cpp module.

## Setup

1. Build the C++ module:
   ```bash
   cd ..
   .\compile.bat
   ```

2. Start SpacetimeDB (if not already running):
   ```bash
   spacetime start
   ```

3. Publish the module:
   ```bash
   cd build
   spacetime publish --project-path .. lib.wasm --clear-database
   ```

4. Generate Rust bindings:
   ```bash
   cd ../client
   spacetime generate --lang rust --out-dir src/module_bindings --project-path .
   ```

5. Run the client:
   ```bash
   cargo run
   ```

## Environment Variables

- `SPACETIMEDB_HOST` - SpacetimeDB host URL (default: `http://localhost:3000`)
- `SPACETIMEDB_DB_NAME` - Database name (default: `sdk-test-procedure-cpp`)

## Procedure Tests

The client will test the following procedures:

- `return_primitive(lhs, rhs)` - Returns sum of two uint32 values
- `return_struct(a, b)` - Returns struct with fields
- `return_constant()` - Returns constant 42
- `will_fail(value)` - Returns error when value=0

## Notes

This is a standalone Rust project, not part of the root workspace.
