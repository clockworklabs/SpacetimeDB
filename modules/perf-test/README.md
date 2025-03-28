# `perf-test` *Rust* test

A module with various `index scan` workloads for SpacetimeDB.

Called as part of our tests to ensure the system is working as expected.

## How to Run

Execute the test `test_index_scans`
at [standalone_integration_test](../../crates/testing/tests/standalone_integration_test.rs):

```bash
cargo test -p spacetimedb-testing test_index_scans
```