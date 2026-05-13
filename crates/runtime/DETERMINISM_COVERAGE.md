# Determinism Coverage

This document tracks which sources of nondeterminism are under control in `spacetimedb-runtime`, which ones are only constrained by current architecture, and which ones still escape the simulator boundary.

It is meant to serve two purposes:

1. Make the current determinism boundary explicit for runtime code, core crates, and DST harnesses.
2. Provide a place to record and review assumptions when a PR changes that boundary.

## Status Definitions

- `Controlled`
  The simulator or runtime owns this source of nondeterminism directly. Given the same seed and the same simulated inputs, behavior should replay the same way.

- `Constrained`
  This surface is not fully simulator-controlled, but the current architecture limits how it is used. Replay should remain stable if those constraints continue to hold.

- `Audited`
  This surface is not mechanically controlled. Current usage has been reviewed and is believed not to affect replay, but that guarantee depends on call patterns and can regress.

- `Known Leak`
  This source can currently escape simulator control and affect replay. It should be treated as explicit technical debt or a documented exception.

- `Out of Scope`
  This crate does not try to control this surface. If it matters for DST, it must be modeled by a higher-level abstraction or test harness.

## Control Matrix

| Surface | Status | Boundary | Current control or assumption | Failure mode if violated | Required direction |
| --- | --- | --- | --- | --- | --- |
| Executor scheduling | Controlled | `runtime::sim::executor` | Runnable selection is driven by seeded simulator RNG | Replay diverges across runs | Keep simulated task scheduling inside the sim executor |
| Simulated task lifecycle | Controlled | `runtime::sim::{executor, JoinHandle}` | Spawn, wake, cancel, and join all happen inside simulator-owned scheduling | Cancellation and join behavior diverge across runs | Keep lifecycle transitions on simulator-owned tasks only |
| Virtual time and timers | Controlled | `runtime::sim::time` | Simulated time advances only through explicit advance or next-timer jump | Timeouts and ordering become host-timing dependent | Keep timer progression fully simulator-owned |
| Runtime RNG and buggify | Controlled | `runtime::sim::rng` | Runtime RNG drives scheduler and probabilistic fault-injection decisions | RNG and fault decisions are not replayable | Keep simulator-owned randomness explicit and seed-driven |
| OS thread creation during simulation | Controlled | `runtime::sim_std` | Unix thread hook rejects `std::thread::spawn` while simulation is active | Host scheduler escapes simulator control | Keep simulated work on simulator tasks, not OS threads |
| OS entropy | Known Leak | `runtime::sim_std` | Randomness requests warn and then delegate to the OS | Same seed can produce different traces | Add backtrace to warnings, remove call sites, eventually fail closed or fully model the source |
| `HashMap` randomized iteration | Audited | Runtime and caller code | Runtime does not force deterministic hash seeding; correctness must not depend on iteration order | Hidden ordering dependencies cause flaky replay | Prefer ordered maps or explicit sorting where observable order matters |
| `tokio::sync` primitives | Constrained | Core crates above runtime | These can be replay-compatible only when all participating tasks remain simulator-owned and progress stays on simulator-controlled async paths | Wake ordering or blocking semantics diverge once code depends on a real runtime or host-driven progress | Audit per primitive and push deep-core paths toward runtime-owned or single-threaded structures |
| `parking_lot::{}` and `std::sync::{}` | Constrained | Core crates, especially datastore | Safe only where access stays single-threaded or non-contended under DST | Host synchronization leaks nondeterministic acquisition order | Keep out of deep-core execution paths; prefer runtime-owned or single-threaded structures |
| File and network I/O | Out of Scope | Runtime crate | Runtime does not simulate filesystem or network behavior | Real I/O timing, ordering, and errors are not replayable | Model via domain-specific DST abstractions |
| Tokio runtime ownership | Constrained | `spacetimedb_runtime::Handle` / shared core APIs | Shared code uses a narrow runtime boundary instead of concrete Tokio subsystems | Concrete Tokio APIs leak into DST-facing core paths | Keep shared code on runtime or domain abstractions, not raw Tokio services |
| Heap allocation and OOM | Known Leak | Broad, especially deep-core direction | Allocation happens through normal Rust paths; deterministic allocation failure is not modeled | Resource-exhaustion behavior is not reproducible | Move the simulation core and eventually deep-core paths toward `no_std + alloc` with explicit allocation boundaries |
| Snapshot / commitlog / datastore host effects | Out of Scope | Higher-level durability and storage layers | Runtime only provides scheduling, time, and fault-decision primitives | Storage semantics depend on real host behavior unless wrapped | Model durable behavior through domain-specific DST abstractions |

## Scope Notes

This document covers the runtime crate and the determinism boundary it exposes to core crates and DST harnesses.

`Controlled` is the target state for nondeterminism surfaces that must participate directly in deterministic simulation testing. `Constrained` and `Audited` are temporary states: they may be acceptable for a period, but they are not strong guarantees. `Known Leak` marks places where replay can still depend on host behavior. `Out of Scope` does not mean unimportant; it means control must live in another layer.

## Update Rule

A PR should update this document if it:

- introduces a new source of nondeterminism,
- changes the control status of an existing surface,
- adds a new assumption about single-threading, iteration order, runtime ownership, or host behavior, or
- removes a leak or upgrades a surface from `Audited` or `Constrained` to `Controlled`.
