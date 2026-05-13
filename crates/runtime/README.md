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
- `node`: node builders and node-local scheduling handles.

[src/sim_std.rs](./src/sim_std.rs) contains hosted glue around simulator:

- `block_on` installs hosted simulation guards for tests.
- `check_determinism` replays same seeded workload twice and compares trace.
- libc randomness hooks warn and delegate if code reaches host entropy.
- Unix thread hooks reject accidental `std::thread::spawn` while simulation is
  active.

Tokio integration is intentionally small and lives directly in
[src/lib.rs](./src/lib.rs).


Feature flags:

- `tokio`: enables hosted runtime backend and remains in default feature set.
- `simulation`: enables deterministic simulation runtime and hosted `sim_std`
  helpers.

## Scope and Limitations

- **Single-threaded runtime.** The simulator exposes interleaving and timeout
  bugs, but not bugs that require true parallel execution. The direction is to
  keep deep-core code single-threaded or close to thread-per-core; simulating
  real parallelism is not planned here.

- **Nodes are not full processes.** Nodes are separate scheduling domains, but
  they still run on one executor. Stronger process boundaries should be
  modeled by higher-level DST harnesses.

- **One shared virtual clock.** Nodes share one clock, so the runtime cannot
  model skew or drift. If a test needs mismatched clocks, the harness should
  model that above this crate.

- **No built-in network, storage, or I/O simulation.** This crate provides
  deterministic execution primitives only. Higher-level harnesses should model
  message delivery, disk behavior, and failures.

- **Not a Tokio replacement.** This crate does not aim to simulate APIs like
  `tokio::net` or `tokio::fs`. Code that depends on them needs a higher-level
  abstraction boundary.

- **`spawn_blocking` is only a facade on simulation.** On the simulation
  backend it currently delegates to a normal spawned task, so the closure
  still runs on the single executor thread and can block runtime progress. The
  direction is to avoid relying on blocking-pool semantics in simulated deep
  core paths.

- **Host randomness is not controlled.** `sim_std` warns and delegates if code
  reaches OS entropy. The direction is to keep deep-core code and DST
  harnesses off host randomness entirely.

- **Not fully `no_std` or allocation-controlled yet.** The simulation core is
  written with a `no_std + alloc` direction in mind, so moving its core
  further in that direction should be straightforward. Today, though, hosted
  glue still depends on `std`, and the runtime still allocates through normal
  Rust container and task paths. Tight control over heap allocation is a
  direction, not something this crate enforces yet.

- **`NodeId` still coexists with `Node`.** The direction is to move callers
  toward `Node` and reduce raw `NodeId` use over time.
