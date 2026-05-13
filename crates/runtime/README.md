# spacetimedb-runtime

`spacetimedb-runtime` is runtime boundary that lets SpacetimeDB core code run
under deterministic simulation testing (DST).

DST runs code inside a deterministic simulator that controls nondeterministic
inputs instead of letting them come directly from host environment. Given same
seed, simulator should produce same trace. When it finds a bug, seed should be
enough to reproduce that bug exactly.

For this to work, code under test must not read clocks, randomness,
scheduling, I/O, or network behavior directly from outer environment. Those
effects need interfaces that production can implement with hosted services and
DST can replace with simulated ones.

This crate provides the execution-control part of that boundary: spawning,
timeouts, virtual time, deterministic randomness, task scheduling, and fault
decisions. Storage, networking, and replication should be modeled through
higher-level abstractions.

## Architecture

[src/lib.rs](./src/lib.rs) exposes `Runtime`, small runtime handle shared code
carries. It has two variants:

- `Runtime::Tokio(TokioHandle)` for hosted execution.
- `Runtime::Simulation(sim::Handle)` for deterministic simulation.

[src/sim](./src/sim) contains simulation core. It is single-threaded and aims
toward `no_std + alloc` over time. This includes:

- `executor`: single-threaded task scheduler with deterministic runnable selection.
- `time`: virtual clock, sleeps, and timeouts.
- `rng`: seeded deterministic randomness for scheduler and workload decisions.
- `buggify`: seeded fault-injection decisions.
- `config`: runtime seed and simulator configuration.

[src/sim_std.rs](./src/sim_std.rs) contains hosted glue around simulator:

- `block_on` installs thread-local simulation context for hosted tests.
- `check_determinism` replays same seeded workload twice and compares trace.
- libc randomness hooks route entropy requests to runtime RNG while simulation
  is active, and warn before delegating to host OS outside simulation.
- Unix thread hooks reject accidental `std::thread::spawn` while simulation is
  active.

Tokio integration is intentionally small and lives directly in
[src/lib.rs](./src/lib.rs).

The crate is intentionally hybrid because SpacetimeDB is hybrid. Host-facing
systems such as networking, subscriptions, wasm host glue, auth, process
metrics, and CLI code may continue to use hosted infrastructure. Deep-core and
DST-facing paths should instead depend on `Runtime` or narrower
domain-specific traits passed in by the caller.

Feature flags:

- `tokio`: enables hosted runtime backend and remains in default feature set.
- `simulation`: enables deterministic simulation runtime and hosted `sim_std`
  helpers.
