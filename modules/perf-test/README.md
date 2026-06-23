# `perf-test` *Rust* benchmark module

A module with various `index scan` workloads for SpacetimeDB.

Called by the `index_scan_gate` benchmark to ensure the system is working as expected.

## How to Run

Execute the benchmark gate:

```bash
cargo bench -p spacetimedb-bench --bench index_scan_gate
```
