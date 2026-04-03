use crate::{
    scheduler::{Actor, ScheduleMode, Scheduler},
    seed::DstSeed,
    trace::Trace,
};

pub fn run_seeded<A: Actor>(actors: Vec<A>, seed: DstSeed) -> Trace<A::Event> {
    Scheduler::new(actors, ScheduleMode::Seeded, Some(seed.rng())).run_to_completion()
}
