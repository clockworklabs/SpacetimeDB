//! Deterministic simulation testing utilities for SpacetimeDB crates.
//!
//! Layout:
//!
//! - Root: harness pieces such as [`seed`], [`trace`], [`subsystem`], and
//!   [`runner`].
//! - Root shared target helpers: [`config`] and [`schema`].
//! - Root generic helpers: [`bugbase`] and [`shrink`].
//! - [`sim`]: reusable simulator primitives such as [`scheduler`] and [`sync`].
//! - [`workload`]: shared workload/model/property generation reused by targets.
//! - [`targets`]: concrete simulation targets such as [`datastore_sim`] and
//!   `relational_db`.
//!
//! Reading guide:
//!
//! - Start with [`subsystem`] to understand the common `Case -> Trace ->
//!   Outcome` shape used across simulations.
//! - Then read [`runner`] for the small orchestration helpers that generate,
//!   run, and replay a case.
//! - Read [`sim`] for reusable simulation building blocks.
//! - Read [`workload`] for shared table-workload planning split into
//!   scenarios, generation, model, and properties.
//! - Then read the concrete targets in [`targets`].
//! - [`config`] and [`schema`] hold reusable target-side data shapes.
//! - [`bugbase`] and [`shrink`] are the debugging path after a failure.
//!
//! The crate is primarily a library crate, but long-running DST workloads are
//! intended to be driven through the `dst` binary via `run`, `replay`, and
//! `shrink` commands.

/// Generic persisted failure artifacts and JSON helpers.
pub mod bugbase;
/// Shared run-budget configuration for DST targets.
pub mod config;
/// Small helpers for generating, running, rerunning, and replay-checking cases.
pub mod runner;
/// Shared schema and row model used by DST targets.
pub mod schema;
/// Stable seed and RNG utilities used to make runs reproducible.
pub mod seed;
/// Generic shrinking helpers.
pub mod shrink;
/// Reusable simulation primitives.
pub mod sim;
/// Common traits and result types shared by DST subsystems.
pub mod subsystem;
/// Concrete simulator targets.
pub mod targets;
/// Trace data structures used to record deterministic execution.
pub mod trace;
/// Shared workload generators reused by multiple targets.
pub mod workload;

/// Generic actor scheduler used by deterministic simulations.
pub use sim::scheduler;
/// Small in-memory synchronization model used by scheduler-oriented tests.
pub use sim::sync;
/// Higher-level randomized datastore simulator with schema and interaction plans.
pub use targets::datastore as datastore_sim;
