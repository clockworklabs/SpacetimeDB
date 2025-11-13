# SpacetimeDB Rust Client

A basic Rust client for SpacetimeDB.

## Setup

1. Build and publish your server module
2. Generate bindings:
   ```
   spacetime generate --lang rust --out-dir src/module_bindings
   ```
3. Run the client:
   ```
   cargo run
   ```
