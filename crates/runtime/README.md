# spacetimedb-runtime

`spacetimedb-runtime` is the runtime boundary shared by SpacetimeDB core code
and deterministic simulation testing (DST).

The goal is deliberately smaller than "make our own Tokio." We do not try to
support `tokio::net`, `tokio::fs`, `tokio::io`, or arbitrary ecosystem runtime
compatibility here. The crate gives core database code the small amount of
execution control it needs so the same code path can run under either a
deterministic single-threaded simulator or a hosted adapter.

That makes the runtime model intentionally hybrid. Networking, subscriptions,
client-facing services, and other integration-heavy infrastructure can stay on
Tokio. Core database paths that DST needs to explore should depend on explicit
runtime and storage abstractions instead. This follows the broader SpacetimeDB
direction: keep core state transitions deterministic and replayable, isolate
side effects behind small domain interfaces, and avoid letting host
infrastructure leak into database semantics.

## Architecture

The top-level type in [src/lib.rs](./src/lib.rs) is `Runtime`. It is the small
facade that shared core code should depend on when it needs to spawn work,
run blocking work, or apply runtime-owned timeouts. `Runtime` is not the
simulator itself and it is not Tokio. It is a tagged handle with the backends
that matter to SpacetimeDB:

- `Runtime::Tokio(TokioHandle)` when the `tokio` feature is enabled
- `Runtime::Simulation(sim::Handle)` when the `simulation` feature is enabled

Code such as durability and snapshotting should accept or store `Runtime` and
use only the narrow operations exposed there. That keeps shared logic
independent of the hosted runtime choice.

Under that facade, this crate has two layers.

The first layer is the simulation core under [src/sim](./src/sim). This is the
deterministic single-thread runtime used by DST. The long-term direction for
this layer is `no_std + alloc`, explicit handles, explicit time, and no
dependency on ambient host facilities.

The second layer is the hosted adapter layer under [src/adapter](./src/adapter).
Today that includes a Tokio adapter and std-hosted simulation conveniences.
Those conveniences are useful for DST running as a normal process, but they are
adapters around the simulation core, not part of the portable core itself.

## Runtime Contract

The runtime contract is about control, not API compatibility. Code that wants
to be runnable under DST should route scheduling, time, randomness, and
runtime-owned background work through this crate or through a domain-specific
abstraction built on top of it.

`Runtime` is the API for shared code. `sim::Runtime` is the deterministic engine
used by simulation tests. `adapter::*` is hosted glue for environments that have
Tokio, std, thread-local convenience APIs, or OS hooks available.

Ambient runtime lookup should stay at the edge. Constructors such as
`Runtime::tokio_current()`, `Runtime::simulation_current()`, and
`current_handle_or_new_runtime()` are useful in bootstrap and adapter code, but
core database code should prefer explicit dependency injection. Passing the
runtime in makes tests replayable and makes the execution boundary visible in
review.

`Runtime::timeout` is also runtime-owned. In the Tokio backend it is a real
Tokio timeout. In the simulation backend it is driven by virtual time. Shared
code should not assume wall-clock behavior unless it is intentionally running
only in a hosted adapter.

## Determinism Boundary

The simulator can only make behavior deterministic when the behavior is under
simulator control. In the simulation backend, the runtime controls:

- task scheduling and runnable selection
- simulated nodes and pause/resume behavior
- virtual time and sleeps
- runtime RNG decisions
- buggify fault decisions tied to the runtime seed
- task lifecycle for futures spawned through the simulation handle

These are reproducible from the runtime seed and the same sequence of simulated
inputs. If a test fails, DST should be able to report the target, scenario,
seed, interaction budget, and fault profile needed to reproduce the failure.

The simulator does not make arbitrary host effects deterministic. Direct use of
OS threads, kernel blocking, wall-clock sleeps, real filesystem behavior,
process randomness, sockets, Tokio reactors, or external services is outside
the deterministic contract. Those effects might still be fine in production,
but DST needs them behind a smaller abstraction with a simulated
implementation.

## How To Write Shared Code

Prefer explicit dependencies. If shared code needs to spawn background work,
accept a `Runtime`. If it needs durable storage, accept a commitlog or snapshot
repository abstraction. If it needs time, accept a runtime or clock abstraction.
If it needs network behavior, accept a logical transport abstraction. Do not
pull in raw `tokio::fs`, `tokio::net`, `tokio::io`, or `tokio::time` from the
middle of a core database path and expect DST to control it later.

The abstraction should match the domain, not the implementation detail. For
commitlog code, abstract over segment/repo operations. For snapshot code,
abstract over snapshot repository and object operations. For future networked
targets, abstract over logical messages and transport behavior. A byte stream
trait is only the right abstraction if byte stream behavior is what the test is
actually trying to model.

For now, some core crates may still use `tokio::sync`. That is tolerated as a
short-term exception because those primitives are not tied to the Tokio reactor
in the same way as `tokio::net`, `tokio::fs`, or `tokio::time`. It should not
be read as permission to spread Tokio types through new DST-facing APIs. The
longer-term direction is to keep core database modules closer to explicit,
runtime-agnostic, and eventually `no_std + alloc`-friendly primitives.

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

This means “simulation” is not shorthand for “all simulation tooling.” It is
the portable runtime core. Hosted extras live behind `simulation-std`, and
Tokio-specific integration lives behind `tokio`.

## Simulation Core

The simulation core lives under [src/sim](./src/sim).

[src/sim/executor.rs](./src/sim/executor.rs) contains the single-threaded
deterministic executor. It stores ready tasks as `async_task` runnables, uses a
deterministic RNG to choose the next runnable, supports pause/resume by logical
node, and treats “no runnable work and no future timer wakeups” as a hang.

[src/sim/time/](./src/sim/time/) contains virtual time. It owns simulated
time state, timer registration, and timeout behavior. The key property is that
time moves only under runtime control, not wall clock control.

[src/sim/rng.rs](./src/sim/rng.rs) contains deterministic randomness. The
runtime uses this for scheduler choices, and test/workload code can use
`Rng`/`GlobalRng` when it needs deterministic probabilistic decisions.

[src/sim/buggify.rs](./src/sim/buggify.rs) contains runtime-owned fault
injection helpers. Buggify is tied to a simulation runtime so fault decisions
come from the same seeded decision stream as the rest of the simulated run.

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

## DST Harness

The DST crate has its own wrapper under `crates/dst/src/sim`. That wrapper keeps
DST-facing types such as `DstSeed` local to the DST crate while delegating
execution to `spacetimedb-runtime`.

DST currently uses `simulation-std` because the harness itself runs as a normal
hosted process. That is where thread-local current-handle access,
determinism-check helpers, std random seeding, and pthread guards belong. The
portable simulation core should not grow `simulation-std` conditionals to make
those conveniences work.

When adding a DST target, route target execution through the DST sim wrapper,
use `--max-interactions` for exact replay, and make all probabilistic choices
come from the run seed or the runtime RNG. Duration-based runs are useful for
local soak testing, but they are not an exact replay budget.

## Current Scope

This crate is not trying to make the whole of core `no_std` immediately. For
now, crates such as relational DB, snapshot, commitlog, and datastore may still
contain std or Tokio-adjacent internals. The first goal is not a full portability
rewrite. The first goal is to stop execution, time, randomness, and durable
effects from being hidden behind ambient host APIs.

Longer term, the same boundary should make it easier to move selected core
database modules toward more constrained dependencies. That likely means more
small domain abstractions, fewer ambient singletons, fewer runtime-specific
types in core APIs, and less reliance on host behavior that DST cannot replay.

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

## Review Checklist

Use this checklist when adding code that should be runnable under DST:

- Does the code receive `Runtime` or a domain abstraction explicitly instead of
  calling an ambient Tokio/simulation handle from the middle of core logic?
- Are sleeps, timeouts, background tasks, randomness, and fault decisions routed
  through runtime-controlled APIs?
- Are filesystem, network, process, and thread effects hidden behind
  domain-level abstractions with deterministic implementations for DST?
- Does the code avoid direct `tokio::fs`, `tokio::net`, `tokio::io`,
  `tokio::time`, `std::thread`, wall-clock time, and process randomness on the
  DST path?
- If `tokio::sync` is used, is it an internal short-term dependency rather than
  a new public boundary for DST-facing core code?
- Can a failure be reproduced from target, scenario, seed, interaction budget,
  and fault profile without relying on wall-clock duration or host scheduling?
