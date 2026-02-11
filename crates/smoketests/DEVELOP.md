# Smoketests Development Guide

## Running Tests

### Recommended: cargo smoketest

```bash
cargo smoketest
```

This command:
1. Builds `spacetimedb-cli` and `spacetimedb-standalone` binaries
2. Runs all smoketests in parallel using nextest (or cargo test if nextest isn't installed)

To run specific tests:
```bash
cargo smoketest test_sql_format
cargo smoketest "cli::"  # Run all CLI tests
```

### WARNING: Stale Binary Risk

**Smoketests use pre-built binaries and DO NOT automatically rebuild them.**

If you modify code in `spacetimedb-cli`, `spacetimedb-standalone`, or their dependencies,
you MUST rebuild before running tests:

```bash
# Option 1: Use cargo smoketest (always rebuilds first)
cargo smoketest

# Option 2: Manually rebuild, then run tests directly
cargo build -p spacetimedb-cli -p spacetimedb-standalone --features spacetimedb-standalone/allow_loopback_http_for_tests
cargo nextest run -p spacetimedb-smoketests
```

**If you run `cargo nextest run` or `cargo test` directly without rebuilding,
you may be testing against OLD binaries.** This can cause confusing test failures
or, worse, tests that pass when they shouldn't.

To check which binary you're testing against:
```bash
ls -la target/debug/spacetimedb-cli*  # Check modification time
```

### Why This Design?

Running `cargo build` from inside parallel tests causes race conditions on Windows
where multiple processes try to replace running executables ("Access denied" errors).
Pre-building avoids this entirely.

### Alternative: cargo test

Standard `cargo test` also works, but you must rebuild first:

```bash
cargo build -p spacetimedb-cli -p spacetimedb-standalone --features spacetimedb-standalone/allow_loopback_http_for_tests
cargo test -p spacetimedb-smoketests
```

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
