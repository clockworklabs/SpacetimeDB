//! Shared traits for deterministic simulation subsystems.
//!
//! A subsystem defines:
//!
//! - a generated `Case`,
//! - a stream of traced `Event`s,
//! - a final `Outcome`.
//!
//! `RunRecord` packages those pieces together so replay checks and invariants
//! can reason about one run without knowing subsystem-specific details.

use crate::{seed::DstSeed, trace::Trace};

/// A deterministic simulation subsystem.
pub trait DstSubsystem {
    type Case: Clone + core::fmt::Debug + Eq + PartialEq;
    type Event: Clone + core::fmt::Debug + Eq + PartialEq;
    type Outcome: Clone + core::fmt::Debug + Eq + PartialEq;

    fn name() -> &'static str;
    fn generate_case(seed: DstSeed) -> Self::Case;
    fn run_case(case: &Self::Case) -> anyhow::Result<RunRecord<Self::Case, Self::Event, Self::Outcome>>;
}

/// Result of one fully executed deterministic run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRecord<C, E, O> {
    /// Human-readable subsystem name used in logs and replay diagnostics.
    pub subsystem: &'static str,
    /// Top-level seed that produced this run.
    pub seed: DstSeed,
    /// Full generated or loaded input case.
    pub case: C,
    /// Optional execution trace collected while the case ran.
    pub trace: Option<Trace<E>>,
    /// Final target-specific outcome after execution completes.
    pub outcome: O,
}

/// Post-run assertion over a run record.
pub trait Invariant<R> {
    fn name(&self) -> &'static str;
    fn check(&self, run: &R) -> anyhow::Result<()>;
}

/// Runs each invariant and annotates failures with the invariant name.
pub fn assert_invariants<R>(run: &R, invariants: &[&dyn Invariant<R>]) -> anyhow::Result<()> {
    for invariant in invariants {
        invariant
            .check(run)
            .map_err(|err| anyhow::anyhow!("invariant `{}` failed: {err}", invariant.name()))?;
    }
    Ok(())
}
