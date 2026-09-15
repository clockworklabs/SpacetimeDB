# Smoketests Development Guide

## Running Tests

### Recommended: cargo smoketest

```bash
cargo smoketest
```

This command:
1. Builds `spacetimedb-cli` and `spacetimedb-standalone` binaries
2. Builds the Rust fixture workspace in `crates/smoketests/modules/` to WASM
3. Runs all smoketests in parallel using nextest (or cargo test if nextest isn't installed)

To run specific tests:
```bash
cargo smoketest test_sql_format
cargo smoketest "cli::"  # Run all CLI tests
```

### Remote Servers

Run against a standalone-compatible remote server with:

```bash
cargo ci smoketests --server https://example.spacetimedb.com
```

Maincloud and maincloud staging require SpacetimeAuth-issued tokens rather than
server-issued tokens. Use `--auth-host` for those:

```bash
cargo ci smoketests --server https://maincloud.staging.spacetimedb.com --auth-host
```

The runner invokes `spacetime login` once, then copies that logged-in config
into each isolated smoketest config. Tests that need throwaway server-issued
identities should call `require_server_issued_login!()` so they skip in
SpacetimeAuth mode.

### WARNING: Stale Binary Risk

**Smoketests use pre-built binaries and DO NOT automatically rebuild them.**

If you modify code in `spacetimedb-cli`, `spacetimedb-standalone`, or their dependencies,
you MUST rebuild before running tests:

```bash
# Option 1: Use cargo smoketest (always rebuilds first)
cargo smoketest

# Option 2: Manually rebuild, then run tests directly
cargo build -p spacetimedb-cli -p spacetimedb-standalone --features spacetimedb-standalone/allow_loopback_http_for_tests
cargo build --manifest-path crates/smoketests/modules/Cargo.toml --workspace --release --target wasm32-unknown-unknown
CARGO_BUILD_PROFILE=debug cargo nextest run -p spacetimedb-smoketests
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
cargo build --manifest-path crates/smoketests/modules/Cargo.toml --workspace --release --target wasm32-unknown-unknown
CARGO_BUILD_PROFILE=debug cargo test -p spacetimedb-smoketests
```

## Test Performance

Rust fixtures are compiled once during warmup and reused across tests. Ordinary
tests then start a server and publish the selected WASM without invoking Cargo.
Tests of build diagnostics explicitly compile temporary modules.

When running tests in parallel, resource contention increases individual test times but reduces overall runtime.

## Writing Tests

Add a fixture crate under `crates/smoketests/modules/`, following an existing
crate's `Cargo.toml` and `src/lib.rs`, and list it in that workspace's members.
The package name `smoketest-module-example` makes it available as `example`:

```rust
use spacetimedb_smoketests::Smoketest;

#[test]
fn test_example() {
    let test = Smoketest::builder()
        .precompiled_module("example")
        .build();

    test.call("add", &["42"]).unwrap();
    test.assert_sql("SELECT * FROM example", "value\n-----\n42");
}
```

Place the table and `add` reducer in the fixture's `src/lib.rs`. Use
`test.use_precompiled_module("example-updated")` to switch fixtures for migration
tests. If no module is selected, publishing uses the precompiled `noop` fixture.
`autopublish(false)` leaves the database unpublished and does not need that fixture
until a publish is requested.

For tests that expect Rust build failures, use `build_rust_module(source, extra_deps)`
and assert the specific diagnostic in its raw output. This helper runs
`spacetime build` without starting a server. Keep ordinary test modules in the
fixture workspace. The `http-handlers-tutorial` fixture shows how a build script
can compile examples directly from current documentation during warmup.
