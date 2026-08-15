use anyhow::Error;
use spacetimedb_runtime::sim::Rng;

/// This should be implemented by System under test.
pub trait TargetDriver<I> {
    type Observation;

    fn execute<'a>(
        &'a mut self,
        interaction: &'a I,
    ) -> impl std::future::Future<Output = Result<Self::Observation, Error>> + 'a;
}

/// Ensures if Output of `TargetDrive` is expected for the input
pub trait Properties<I, O> {
    fn observe(&mut self, interaction: &I, observation: &O) -> Result<(), Error>;
}

/// Generates interactions, and can feed observations back to the generator so
/// its internal state stays in sync with the target.
pub trait InteractionGen<O>: std::fmt::Debug {
    type Interaction: std::fmt::Debug;

    fn next_interaction(&mut self) -> Self::Interaction;

    /// Feed an observation back to the generator. Defaults to ignoring it.
    fn observe(&mut self, _interaction: &Self::Interaction, _observation: &O) -> Result<(), Error> {
        Ok(())
    }
}

pub type TestSuiteParts<S> = (
    <S as TestSuite>::Interactions,
    <S as TestSuite>::Target,
    <S as TestSuite>::Properties,
);

pub trait TestSuite {
    type Interaction: std::fmt::Debug;
    type Interactions: InteractionGen<
        <Self::Target as TargetDriver<Self::Interaction>>::Observation,
        Interaction = Self::Interaction,
    >;
    type Target: TargetDriver<Self::Interaction>;
    type Properties: Properties<Self::Interaction, <Self::Target as TargetDriver<Self::Interaction>>::Observation>;

    fn build(&self, rng: Rng) -> impl std::future::Future<Output = Result<TestSuiteParts<Self>, Error>> + '_
    where
        Self: Sized;

    fn run(&self, rng: Rng, max_interactions: usize) -> impl std::future::Future<Output = Result<(), Error>> + '_
    where
        Self: Sized,
    {
        async move {
            let (mut interactions, mut target, mut properties) = self.build(rng).await?;

            for _ in 0..max_interactions {
                let interaction = interactions.next_interaction();
                let observation = target.execute(&interaction).await?;
                interactions.observe(&interaction, &observation)?;
                properties.observe(&interaction, &observation)?;
            }

            tracing::info!(interaction_counts = ?interactions, "final interaction counts");

            Ok(())
        }
    }
}
