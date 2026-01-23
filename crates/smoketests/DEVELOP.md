# Smoketests Development Guide

## Running Tests

### Recommended: cargo-nextest

For faster test execution, use [cargo-nextest](https://nexte.st/):

```bash
# Install (one-time)
cargo install cargo-nextest --locked

# Run all smoketests
cargo nextest run -p spacetimedb-smoketests

# Run a specific test
cargo nextest run -p spacetimedb-smoketests test_sql_format
```

**Why nextest?** Standard `cargo test` compiles each test file in `tests/` as a separate binary and runs them sequentially. Nextest runs all test binaries in parallel, reducing total runtime by ~40% (160s vs 265s for 25 tests).

### Alternative: cargo test

Standard `cargo test` also works:

```bash
cargo test -p spacetimedb-smoketests
```

Tests within each file run in parallel, but files run sequentially.

## Test Performance

Each test takes ~15-20s due to:
- **WASM compilation** (~12s): Each test compiles a fresh Rust module to WASM
- **Server spawn** (~2s): Each test starts its own SpacetimeDB server
- **Module publish** (~2s): Server processes and initializes the WASM module

When running tests in parallel, resource contention increases individual test times but reduces overall runtime.

## Writing Tests

See existing tests for patterns. Key points:

```rust
use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = example, public)]
pub struct Example { value: u64 }

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, value: u64) {
    ctx.db.example().insert(Example { value });
}
"#;

#[test]
fn test_example() {
    let test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .build();

    test.call("add", &["42"]).unwrap();
    test.assert_sql("SELECT * FROM example", "value\n-----\n42");
}
```
