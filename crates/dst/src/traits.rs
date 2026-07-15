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

pub type TestSuiteParts<S> = (
    <S as TestSuite>::Interactions,
    <S as TestSuite>::Target,
    <S as TestSuite>::Properties,
);

pub trait TestSuite {
    type Interaction: std::fmt::Debug;
    type Interactions: Iterator<Item = Self::Interaction> + std::fmt::Debug;
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

            let result = async {
                for interaction in interactions.by_ref().take(max_interactions) {
                    let observation = target.execute(&interaction).await?;
                    properties.observe(&interaction, &observation)?;
                }

                Ok(())
            }
            .await;

            tracing::info!(interaction_counts = ?interactions, "final interaction counts");

            result
        }
    }
}
