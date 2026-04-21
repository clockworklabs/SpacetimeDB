//! Deterministic simulation testing utilities for SpacetimeDB crates.
//!
//! Layout:
//!
//! - Root: harness pieces such as [`seed`], [`trace`], [`subsystem`], and
//!   [`runner`].
//! - Root generic helpers: [`bugbase`] and [`shrink`].
//! - [`sim`]: reusable simulator primitives such as [`scheduler`] and [`sync`].
//! - [`targets`]: concrete simulation targets such as [`datastore_sim`].
//!
//! Reading guide:
//!
//! - Start with [`subsystem`] to understand the common `Case -> Trace ->
//!   Outcome` shape used across simulations.
//! - Then read [`runner`] for the small orchestration helpers that generate,
//!   run, and replay a case.
//! - Read [`sim`] for reusable simulation building blocks.
//! - For the datastore simulator itself, read [`datastore_sim`] top-down:
//!   case format, generator, executor, then the expected-state model used by
//!   the final consistency check.
//! - [`bugbase`] and [`shrink`] are the debugging path after a failure.
//!
//! The crate is intentionally a library crate. It exposes reusable pieces for
//! tests and future binaries rather than providing a CLI directly.

/// Generic persisted failure artifacts and JSON helpers.
pub mod bugbase;
/// Small helpers for generating, running, rerunning, and replay-checking cases.
pub mod runner;
/// Stable seed and RNG utilities used to make runs reproducible.
pub mod seed;
/// Common traits and result types shared by DST subsystems.
pub mod subsystem;
/// Trace data structures used to record deterministic execution.
pub mod trace;
/// Generic shrinking helpers.
pub mod shrink;
/// Reusable simulation primitives.
pub mod sim;
/// Concrete simulator targets.
pub mod targets;

/// Higher-level randomized datastore simulator with schema and interaction plans.
pub use targets::datastore as datastore_sim;
/// Generic actor scheduler used by deterministic simulations.
pub use sim::scheduler;
/// Small in-memory synchronization model used by scheduler-oriented tests.
pub use sim::sync;
