use crate::{
    scheduler::{Actor, ScheduleMode, Scheduler},
    seed::DstSeed,
    subsystem::{DstSubsystem, RunRecord},
    trace::Trace,
};

pub fn run_seeded<A: Actor>(actors: Vec<A>, seed: DstSeed) -> Trace<A::Event> {
    Scheduler::new(actors, ScheduleMode::Seeded, Some(seed.rng())).run_to_completion()
}

pub fn run_generated<S: DstSubsystem>(seed: DstSeed) -> anyhow::Result<RunRecord<S::Case, S::Event, S::Outcome>> {
    let case = S::generate_case(seed);
    S::run_case(&case)
}

pub fn rerun_case<S: DstSubsystem>(
    record: &RunRecord<S::Case, S::Event, S::Outcome>,
) -> anyhow::Result<RunRecord<S::Case, S::Event, S::Outcome>> {
    S::run_case(&record.case)
}

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
