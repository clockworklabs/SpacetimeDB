# `benchmarks` *Rust* Modules

This provides the various modules used for benchmarking SpacetimeDB, with the crate
[`bench`](../../crates/bench).

> **Note:** Also mirrored as a C# version at [`modules/benchmarks-cs`](../benchmarks-cs/), so must be kept in sync.

## Benchmarks

### [`circles.rs`](src/circles.rs)

Implements a smaller variation of the [Blackholio](https://github.com/clockworklabs/Blackholio) game.

Circles are spawned and then queried to evaluate `CROSS JOIN` performance.

### [`ia_loop.rs`](src/ia_loop.rs)

Implements a simplified version of the `ia` loop from [BitCraft](https://bitcraftonline.com/).

This benchmark spawns a large number of entities in a `world` and queries them to measure `UPDATE` performance, running
a single loop of the `enemy` AI.

### [`synthetic.rs`](src/synthetic.rs)

Contains various synthetic benchmarks designed to test database performance. These benchmarks involve tables with
different `type` combinations and evaluate `INSERT`, `UPDATE`, `DELETE`, and `SELECT` operations in both simple and bulk
scenarios.

## How to Run

For detailed instructions on running the benchmarks, refer to the [benchmarks README](../../crates/bench/README.md).  