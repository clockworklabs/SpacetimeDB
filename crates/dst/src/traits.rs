use anyhow::Error;
use spacetimedb_runtime::sim::Rng;

/// This should be implemented by System under test.
pub trait TargetDriver<I> {
    type Observation;
    type Outcome;

    fn execute(&mut self, interaction: &I) -> Result<Self::Observation, Error>;
}

/// Ensures if Output of `TargetDrive` is expected for the input
pub trait Properties<I, O> {
    fn observe(&mut self, interaction: &I, observation: &O) -> Result<(), Error>;
}

pub trait TestSuite {
    type Interaction;
    type Interactions: Iterator<Item = Self::Interaction> + std::fmt::Debug;
    type Target: TargetDriver<Self::Interaction>;
    type Properties: Properties<Self::Interaction, <Self::Target as TargetDriver<Self::Interaction>>::Observation>;

    fn build(&self, rng: Rng) -> Result<(Self::Interactions, Self::Target, Self::Properties), Error>;

    fn run(&self, rng: Rng, max_interactions: Option<usize>) -> Result<(), Error>
    where
        Self: Sized,
    {
        let (mut interactions, mut target, mut properties) = self.build(rng)?;

        let result = (|| {
            for interaction in interactions.by_ref().take(max_interactions.unwrap_or(usize::MAX)) {
                let observation = target.execute(&interaction)?;
                properties.observe(&interaction, &observation)?;
            }

            Ok(())
        })();

        tracing::info!(interaction_counts = ?interactions, "final interaction counts");

        result
    }
}
