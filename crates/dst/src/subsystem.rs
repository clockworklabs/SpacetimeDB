use crate::{seed::DstSeed, trace::Trace};

pub trait DstSubsystem {
    type Case: Clone + core::fmt::Debug + Eq + PartialEq;
    type Event: Clone + core::fmt::Debug + Eq + PartialEq;
    type Outcome: Clone + core::fmt::Debug + Eq + PartialEq;

    fn name() -> &'static str;
    fn generate_case(seed: DstSeed) -> Self::Case;
    fn run_case(case: &Self::Case) -> anyhow::Result<RunRecord<Self::Case, Self::Event, Self::Outcome>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRecord<C, E, O> {
    pub subsystem: &'static str,
    pub seed: DstSeed,
    pub case: C,
    pub trace: Option<Trace<E>>,
    pub outcome: O,
}

pub trait Invariant<R> {
    fn name(&self) -> &'static str;
    fn check(&self, run: &R) -> anyhow::Result<()>;
}

pub fn assert_invariants<R>(run: &R, invariants: &[&dyn Invariant<R>]) -> anyhow::Result<()> {
    for invariant in invariants {
        invariant
            .check(run)
            .map_err(|err| anyhow::anyhow!("invariant `{}` failed: {err}", invariant.name()))?;
    }
    Ok(())
}
