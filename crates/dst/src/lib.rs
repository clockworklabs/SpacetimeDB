//! Deterministic simulation testing utilities for SpacetimeDB crates.
//!
//! Public surface is intentionally narrow and centered on the CLI:
//!
//! - [`client`] for logical client/session identifiers,
//! - [`config`] for run budgets,
//! - [`properties`] for reusable semantic checks,
//! - [`seed`] for deterministic seeds,
//! - [`workload`] for scenario identifiers,
//! - [`targets`] for executable relational-db / standalone-host adapters.
//!
//! ## DST principles
//!
//! 1. Every generated choice comes from [`seed::DstSeed`] or a simulator-provided
//!    deterministic source. A failing run should be replayable from the printed
//!    seed and CLI arguments. Use `--max-interactions` for exact replay; duration
//!    budgets are wall-clock soak limits.
//! 2. Workloads describe legal but stressful user behavior. Targets may add
//!    faults and lifecycle disruption, but the generator should not depend on
//!    target internals.
//! 3. Oracles should check observable state, not merely absence of panics. When
//!    possible, compare the target against a simple model or a replayed durable
//!    history.
//! 4. Keep generation, execution, and property checking separate. This makes it
//!    clear whether a failure came from an invalid workload, a target bug, or a
//!    weak assertion.
//! 5. Prefer streaming state machines over precomputed traces. DST runs should
//!    scale by budget and duration without materializing the whole workload.
//! 6. Fault injection must be explicit, configurable, and summarized in the run
//!    output. Profiles should start with recoverable API-level behavior before
//!    introducing crash or corruption semantics.
//! 7. Shared randomness, weighting, and sampling helpers belong in the
//!    workload strategy module, not in ad hoc target or scenario code.

/// Logical client/session identifiers shared by workloads and targets.
pub mod client;
/// Shared run-budget configuration for DST targets.
pub mod config;
/// Core traits/runners for pluggable workloads and targets.
pub mod core;
/// Reusable semantic properties and expected-model checks.
pub(crate) mod properties;
mod schema;
/// Stable seed and RNG utilities used to make runs reproducible.
pub mod seed;
/// Concrete simulator targets.
pub mod targets;
/// Shared workload generators reused by multiple targets.
pub mod workload;
