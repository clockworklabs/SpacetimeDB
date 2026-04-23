//! Deterministic simulation testing utilities for SpacetimeDB crates.
//!
//! Public surface is intentionally narrow and centered on the CLI:
//!
//! - [`config`] for run budgets,
//! - [`seed`] for deterministic seeds,
//! - [`workload`] for scenario identifiers,
//! - [`targets`] for the executable datastore / relational-db adapters.

/// Shared run-budget configuration for DST targets.
pub mod config;
/// Core traits/runners for pluggable workloads and targets.
pub mod core;
mod schema;
/// Stable seed and RNG utilities used to make runs reproducible.
pub mod seed;
/// Concrete simulator targets.
pub mod targets;
/// Shared workload generators reused by multiple targets.
pub mod workload;
