# spacetimedb-runtime

`spacetimedb-runtime` is the runtime boundary shared by SpacetimeDB core code
and DST. The goal is not to emulate all of Tokio. We do not aim to support
`tokio::net`, `tokio::fs`, or arbitrary ecosystem compatibility here. The goal
is much narrower: provide the small amount of execution control that core
database code needs so that it can run under either a deterministic single-
threaded runtime or a hosted adapter.

The crate is intentionally hybrid. Some parts of the process are naturally
Tokio-owned today, especially networking, subscriptions, and other integration-
heavy infrastructure. DST and selected core/database paths need a different
model: single-threaded, deterministic scheduling, explicit time, and a runtime
that can move toward `no_std + alloc`. This crate exists to support both
execution domains without forcing the whole process onto one scheduler.

## Architecture

The top-level type in [src/lib.rs](./src/lib.rs) is `Runtime`. It is the small
facade that shared core code should depend on. `Runtime` is not the simulator
itself and it is not Tokio. It is a tagged handle with the backends that matter
to SpacetimeDB:

- `Runtime::Tokio(TokioHandle)` when the `tokio` feature is enabled
- `Runtime::Simulation(sim::Handle)` when the `simulation` feature is enabled

Code such as durability and snapshotting should accept or store `Runtime` and
use only the narrow operations exposed there: `spawn`, `spawn_blocking`, and
`timeout`. That keeps shared logic independent of the hosted runtime choice.

Under that facade, this crate has two layers.

The first layer is the simulation core under [src/sim](./src/sim). This is the
deterministic single-thread runtime used by DST. The long-term direction for
this layer is `no_std + alloc`, explicit handles, explicit time, and no
dependency on ambient host facilities.

The second layer is the hosted adapter layer under [src/adapter](./src/adapter).
Today that includes a Tokio adapter and std-hosted simulation conveniences. The
Tokio adapter exists because some production and testing paths still need a real
process runtime. The std-hosted simulation helpers exist because determinism
testing, thread-local convenience APIs, and Unix hooks are useful in hosted
environments even though they are not part of the portable simulation core.

## Feature Model

The crate is organized around features that reflect that layering.

- `simulation`
  Enables the deterministic simulation runtime core. This is the part that is
  intended to move toward `no_std + alloc`.
- `simulation-std`
  Enables std-hosted conveniences layered on top of `simulation`, such as
  thread-local current-handle access, determinism replay helpers, and host OS
  integration hooks used by DST in a normal process.
- `tokio`
  Enables the Tokio-backed hosted adapter and remains part of the default
  feature set for now.
- `std`
  Enables hosted-only functionality shared by the adapter layer.

This means “simulation” is not shorthand for “all simulation tooling.” It is
the portable runtime core. Hosted extras live behind `simulation-std`.

## Simulation Core

The simulation core lives under [src/sim](./src/sim).

[src/sim/executor.rs](./src/sim/executor.rs) contains the single-threaded
deterministic executor. It stores ready tasks as `async_task` runnables, uses a
deterministic RNG to choose the next runnable, supports pause/resume by logical
node, and treats “no runnable work and no future timer wakeups” as a hang.

[src/sim/time.rs](./src/sim/time.rs) contains virtual time. It owns simulated
time state, timer registration, and timeout behavior. The key property is that
time moves only under runtime control, not wall clock control.

[src/sim/rng.rs](./src/sim/rng.rs) contains deterministic randomness. The
runtime uses this for scheduler choices, and test/workload code can use
`DecisionSource` when it needs deterministic probabilistic decisions.

The public simulation surface is intentionally explicit: `sim::Runtime`,
`sim::Handle`, `sim::NodeId`, `sim::JoinHandle`, `yield_now`, and the virtual
time and RNG utilities. The portable direction is to make explicit-handle APIs
the main interface, with host-style convenience APIs layered separately.

## Adapter Layer

The adapter layer lives under [src/adapter](./src/adapter).

[src/adapter/tokio.rs](./src/adapter/tokio.rs) is the Tokio facade. It defines
the hosted Tokio types used by the top-level runtime facade and provides
`current_handle_or_new_runtime()` for production code that may or may not
already be inside a Tokio runtime.

Std-hosted simulation helpers stay outside the simulation core as well. These
helpers are valuable, but they are adapters around the core, not the core
itself. Examples include thread-local “current runtime” access, determinism
replay helpers, and Unix hooks that prevent simulation from silently escaping
onto real OS threads.

## Current Scope

This crate is not trying to make the whole of core `no_std` immediately. For
now, crates such as `relational_db`, `snapshot`, `commitlog`, and `datastore`
may still use `tokio::sync` internally. That is acceptable in the short term,
because those synchronization primitives are runtime-agnostic enough for DST and
the current runtime boundary effort is focused on execution control, not total
removal of Tokio-adjacent types from core.

The longer-term goal is to reduce those dependencies where it materially helps
portability or determinism, but that work is explicitly out of scope for the
first phase of this crate architecture.

## Intended Usage

Shared core/database code should depend on `Runtime`, not on raw Tokio handles
or simulator internals. DST should construct `sim::Runtime` directly and use it
to drive deterministic test execution. Hosted production/testing code that still
needs Tokio should use the Tokio adapter through `Runtime::tokio(...)`,
`Runtime::tokio_current()`, and `current_handle_or_new_runtime()`.

The likely end state is still hybrid: core/database execution may eventually run
on the same deterministic single-thread runtime in both DST and selected
production paths, while networking, clients, subscriptions, and other hosted
subsystems continue to live on Tokio. That is a deliberate design choice, not a
temporary inconsistency.
