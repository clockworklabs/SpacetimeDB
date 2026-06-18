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
    type Interactions: Iterator<Item = Self::Interaction>;
    type Target: TargetDriver<Self::Interaction>;
    type Properties: Properties<Self::Interaction, <Self::Target as TargetDriver<Self::Interaction>>::Observation>;

    fn build(&self, rng: Rng) -> Result<(Self::Interactions, Self::Target, Self::Properties), Error>;

    fn run(&self, rng: Rng) -> Result<(), Error>
    where
        Self: Sized,
    {
        let (interactions, mut target, mut properties) = self.build(rng)?;

        for interaction in interactions {
            let observation = target.execute(&interaction)?;
            properties.observe(&interaction, &observation)?;
        }

        Ok(())
    }
}
