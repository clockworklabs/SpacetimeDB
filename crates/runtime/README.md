# spacetimedb-runtime

`spacetimedb-runtime` is the small runtime abstraction layer shared by core
code and DST. It exists for one reason: code such as durability and
snapshotting needs to spawn work, run blocking sections, and wait with
timeouts, but we want that same code to run on either:

- real Tokio in production, or
- the deterministic DST simulator in tests.

The crate keeps that boundary narrow. Most callers should depend on
`RuntimeDispatch` instead of reaching directly for Tokio or simulator internals.

## Top-level API

The top-level module in [src/lib.rs](./src/lib.rs) exposes:

- `RuntimeDispatch`
  A small tagged runtime handle with two backends:
  - `Tokio(tokio::runtime::Handle)` when the `tokio` feature is enabled
  - `Simulation(sim::Handle)` when the `simulation` feature is enabled
- `spawn(...)`
  Fire-and-forget task spawning.
- `spawn_blocking(...)`
  Run blocking work on the runtime-appropriate backend.
  On Tokio this uses `tokio::task::spawn_blocking`.
  In simulation this is still scheduled through the simulator so ordering stays
  deterministic.
- `timeout(...)`
  Runtime-relative timeout handling.
  On Tokio this uses `tokio::time::timeout`.
  In simulation this uses virtual time from `sim::time`.
- `current_handle_or_new_runtime()`
  Tokio convenience for production code that may or may not already be inside a
  Tokio runtime.

The design goal is intentionally modest: this crate is not a general async
framework. It is a compatibility layer for the small set of runtime operations
SpacetimeDB core code actually needs.

## Features

The crate has two independent backends:

- `tokio`
  Enables production runtime support and is part of the default feature set.
- `simulation`
  Enables the deterministic local simulation runtime used by DST.

Code can compile with one or both features enabled. `RuntimeDispatch` exposes
only the backends that were actually compiled in.

## Simulation Modules

The simulation backend lives under [src/sim](./src/sim).

### `sim::mod`

[src/sim/mod.rs](./src/sim/mod.rs) is the façade for the deterministic runtime.
It re-exports the main executor types and keeps the public surface small:

- `Runtime`
  Owns the simulator executor.
- `Handle`
  Cloneable access to that executor from spawned tasks.
- `NodeId`
  Logical node identifier used to group and pause/resume work.
- `JoinHandle`
  Awaitable handle for spawned simulated tasks.
- `yield_now`
  Cooperative yield point inside the simulator.
- `time`
  Virtual time utilities.
- `Rng` and `DecisionSource`
  Deterministic randomness primitives.

It also exposes small helpers such as `advance_time(...)` and
`decision_source(...)`.

### `sim::executor`

[src/sim/executor.rs](./src/sim/executor.rs) is the heart of the simulator.

It provides a single-threaded async executor adapted from madsim's task loop:

- tasks are stored as `async_task` runnables
- ready work is chosen by a deterministic RNG instead of an OS/runtime scheduler
- node state can be paused and resumed
- a thread-local handle context makes the current simulation runtime accessible
  from inside spawned work
- determinism can be checked by replaying the same future twice and comparing
  the sequence of scheduler decisions

Important behavior:

- `Runtime::block_on(...)` drives the whole simulation
- `Handle::spawn_on(...)` schedules work onto a logical node
- absence of runnable work and absence of future timer wakeups is treated as a
  hang, which is exactly what DST wants

This module is the reason `RuntimeDispatch::Simulation` can behave like a real
runtime without giving up reproducibility.

### `sim::time`

[src/sim/time.rs](./src/sim/time.rs) implements virtual time.

It provides:

- `now()`
  Current simulated time.
- `sleep(duration)`
  A future that completes when simulated time reaches the deadline.
- `timeout(duration, future)`
  Race a future against simulated time.
- `advance(duration)`
  Move time forward explicitly.

Internally it maintains:

- a current `Duration`
- timer registrations keyed by deadline
- wakeups for due timers

The executor uses this module to move time only when necessary, which keeps
tests deterministic and avoids tying correctness to wall-clock behavior.

### `sim::rng`

[src/sim/rng.rs](./src/sim/rng.rs) provides deterministic randomness.

There are two layers:

- `Rng`
  Stateful deterministic RNG used by the executor and runtime internals.
- `DecisionSource`
  Small lock-free source for probabilistic choices in test/workload code.

This module also does two extra jobs:

- records and checks determinism checkpoints so repeated seeded runs can prove
  they took the same execution path
- hooks libc randomness calls such as `getrandom` so code running inside the
  simulator sees deterministic randomness instead of ambient system entropy

That second point matters because reproducibility falls apart quickly if a
dependency reads randomness outside the simulator's control.

### `sim::system_thread`

[src/sim/system_thread.rs](./src/sim/system_thread.rs) prevents accidental OS
thread creation while running under simulation.

On Unix it intercepts `pthread_attr_init` and fails fast if code tries to spawn
real system threads from inside the simulator. That protects determinism and
enforces the intended execution model: simulated tasks should run on the
simulator, not escape onto real threads.

## How This Crate Is Intended To Be Used

For core code:

- accept or store `RuntimeDispatch`
- use `spawn`, `spawn_blocking`, and `timeout`
- avoid embedding raw Tokio assumptions into shared logic

For production-only code:

- use `RuntimeDispatch::tokio_current()` or `RuntimeDispatch::tokio(handle)`

For DST:

- create `sim::Runtime`
- run the test harness with `Runtime::block_on(...)`
- pass `RuntimeDispatch::simulation_current()` into the code under test

## Current Scope

This crate is intentionally narrow. It is not trying to replace Tokio, and it
is not a generic distributed simulator. It currently provides exactly the
runtime seams needed by SpacetimeDB components that must run both in production
and under deterministic simulation.
