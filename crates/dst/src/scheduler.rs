use crate::{seed::DstRng, trace::Trace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepState {
    Progressed,
    Blocked,
    Complete,
}

pub trait Actor {
    type Event: Clone;

    fn step(&mut self, trace: &mut Trace<Self::Event>) -> StepState;
    fn is_complete(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleMode {
    RoundRobin,
    Seeded,
}

pub struct Scheduler<A: Actor> {
    actors: Vec<A>,
    cursor: usize,
    rng: Option<DstRng>,
    trace: Trace<A::Event>,
}

impl<A: Actor> Scheduler<A> {
    pub fn new(actors: Vec<A>, mode: ScheduleMode, rng: Option<DstRng>) -> Self {
        let rng = match mode {
            ScheduleMode::RoundRobin => None,
            ScheduleMode::Seeded => Some(rng.expect("seeded mode requires rng")),
        };
        Self {
            actors,
            cursor: 0,
            rng,
            trace: Trace::default(),
        }
    }

    pub fn run_to_completion(mut self) -> Trace<A::Event> {
        while self.step_once() {}
        self.trace
    }

    pub fn step_once(&mut self) -> bool {
        let runnable = self.runnable_indices();
        if runnable.is_empty() {
            return false;
        }

        let pick = if let Some(rng) = &mut self.rng {
            runnable[rng.index(runnable.len())]
        } else {
            let pick = runnable[self.cursor % runnable.len()];
            self.cursor = self.cursor.wrapping_add(1);
            pick
        };

        !matches!(self.actors[pick].step(&mut self.trace), StepState::Complete)
            || self.actors.iter().any(|actor| !actor.is_complete())
    }

    fn runnable_indices(&self) -> Vec<usize> {
        self.actors
            .iter()
            .enumerate()
            .filter_map(|(idx, actor)| (!actor.is_complete()).then_some(idx))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::trace::Trace;

    use super::{Actor, ScheduleMode, Scheduler, StepState};

    #[derive(Clone)]
    struct CounterActor {
        label: &'static str,
        remaining: usize,
    }

    impl Actor for CounterActor {
        type Event = &'static str;

        fn step(&mut self, trace: &mut Trace<Self::Event>) -> StepState {
            if self.remaining == 0 {
                return StepState::Complete;
            }
            trace.push(self.label);
            self.remaining -= 1;
            if self.remaining == 0 {
                StepState::Complete
            } else {
                StepState::Progressed
            }
        }

        fn is_complete(&self) -> bool {
            self.remaining == 0
        }
    }

    #[test]
    fn round_robin_scheduler_is_stable() {
        let trace = Scheduler::new(
            vec![
                CounterActor {
                    label: "a",
                    remaining: 2,
                },
                CounterActor {
                    label: "b",
                    remaining: 2,
                },
            ],
            ScheduleMode::RoundRobin,
            None,
        )
        .run_to_completion();
        assert_eq!(trace.as_slice(), &["a", "b", "a", "b"]);
    }
}
