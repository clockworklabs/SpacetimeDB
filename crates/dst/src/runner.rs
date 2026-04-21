//! Minimal orchestration helpers for deterministic subsystems.
//!
//! These helpers intentionally stay thin:
//!
//! - generate a case from a seed,
//! - run it,
//! - rerun the exact same case,
//! - compare trace and outcome for replayability.

use crate::{
    scheduler::{Actor, ScheduleMode, Scheduler},
    seed::DstSeed,
    subsystem::{DstSubsystem, RunRecord},
    trace::Trace,
};

/// Runs generic actors under the seeded scheduler and returns the trace.
pub fn run_seeded<A: Actor>(actors: Vec<A>, seed: DstSeed) -> Trace<A::Event> {
    Scheduler::new(actors, ScheduleMode::Seeded, Some(seed.rng())).run_to_completion()
}

/// Generates a case from `seed` and executes it once.
pub fn run_generated<S: DstSubsystem>(seed: DstSeed) -> anyhow::Result<RunRecord<S::Case, S::Event, S::Outcome>> {
    let case = S::generate_case(seed);
    S::run_case(&case)
}

/// Re-executes the exact case stored in a previous run record.
pub fn rerun_case<S: DstSubsystem>(
    record: &RunRecord<S::Case, S::Event, S::Outcome>,
) -> anyhow::Result<RunRecord<S::Case, S::Event, S::Outcome>> {
    S::run_case(&record.case)
}

/// Re-executes a run and checks that both trace and outcome match.
pub fn verify_repeatable_execution<S: DstSubsystem>(
    record: &RunRecord<S::Case, S::Event, S::Outcome>,
) -> anyhow::Result<RunRecord<S::Case, S::Event, S::Outcome>> {
    let replayed = S::run_case(&record.case)?;

    if replayed.trace != record.trace {
        anyhow::bail!(
            "repeatability trace mismatch for subsystem `{}`:\nexpected: {:?}\nactual:   {:?}",
            record.subsystem,
            record.trace.as_ref().map(|trace| trace.as_slice()),
            replayed.trace.as_ref().map(|trace| trace.as_slice())
        );
    }

    if replayed.outcome != record.outcome {
        anyhow::bail!(
            "outcome replay mismatch for subsystem `{}`:\nexpected: {:?}\nactual:   {:?}",
            record.subsystem,
            record.outcome,
            replayed.outcome
        );
    }

    Ok(replayed)
}
