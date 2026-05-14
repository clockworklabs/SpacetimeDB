# spacetimedb-runtime

`spacetimedb-runtime` is a runtime boundary that lets SpacetimeDB core code run under deterministic simulation testing (DST).

DST runs code inside a deterministic simulator that controls nondeterministic inputs instead of letting them come directly from the OS and real runtime environment. Given the same seed, the simulator should produce the same trace. When it finds a bug, the seed should be enough to reproduce that bug exactly.

For this to work, code under test must not read clocks, randomness, scheduling, I/O, or network behavior directly from the outer environment. Those effects need interfaces that production can implement with real runtime-backed services and DST can replace with simulated ones.

This crate provides the execution-control part of that boundary: spawning, timeouts, virtual time, deterministic randomness, task scheduling, and fault decisions. Storage, networking, and replication should be modeled through higher-level abstractions.

## Architecture

[src/lib.rs](./src/lib.rs) exposes `Handle`, a small runtime handle shared code carries. It has two variants:

- `Handle::Tokio(TokioHandle)` for real runtime execution.
- `Handle::Simulation(sim::Handle)` for deterministic simulation.

[src/sim](./src/sim) contains the simulation core. It is single-threaded and targets `no_std + alloc`. The module includes:

- `executor`: single-threaded task scheduler with deterministic runnable selection.
- `time`: virtual clock, sleeps, and timeouts.
- `rng`: seeded deterministic randomness for scheduler and workload decisions.
- `buggify`: fault-injection surface. Calls rng to decide probabilistically whether to inject failures into simulated operations.
- `node`: node builders and node-local scheduling handles.

[src/sim_std.rs](./src/sim_std.rs) contains `std`/OS glue around the simulator:

- `block_on` installs simulation guards for tests running in a normal process.
- `check_determinism` replays the same seeded workload twice and compares traces.
- libc randomness hooks warn and delegate if code reaches OS entropy.
- Unix thread hooks reject accidental `std::thread::spawn` while simulation is active.

Tokio integration is intentionally small and lives directly in [src/lib.rs](./src/lib.rs).

Feature flags:

- `tokio`: enables the Tokio runtime backend and remains in the default feature set.
- `simulation`: enables the deterministic simulation runtime and `sim_std` helpers.

## Related documents

- **[DETERMINISM_COVERAGE.md](./DETERMINISM_COVERAGE.md)** — tracks nondeterminism surfaces.

## Design Principles

- **Single-threaded runtime.** The simulator exposes interleaving and timeout bugs, but not bugs that require true parallel execution. The direction is to keep deep-core code single-threaded or close to thread-per-core; simulating real parallelism is out of scope.

- **No built-in network, storage, or I/O simulation.** This crate provides deterministic execution primitives only. Higher-level harnesses should model message delivery, disk behavior, and failures.

- **Not a Tokio replacement.** This crate does not aim to simulate APIs like `tokio::net` or `tokio::fs`. Code that depends on them needs a higher-level abstraction boundary.

- **Zero dependency.** The simulation core in `sim/` is already `no_std + alloc`. The `sim_std` module is a thin OS-facing wrapper — the std dependency lives there, not in the simulation core itself. It stays until the application logic above this crate also moves to `no_std`.

## Current Limitations


- **One shared virtual clock.** All simulated nodes share a single clock. This masks bugs related to timing mismatch across machines.

- **`spawn_blocking` is only a facade on the simulation backend.** It delegates to a normal spawned task, so the closure still runs on the single executor thread and can block runtime progress. The direction is to avoid relying on blocking-pool semantics.

- **OS randomness is not controlled.** `sim_std` warns if code reaches OS entropy. The direction is to keep application code and testing harnesses off OS randomness entirely.
