use anyhow::Error;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait TargetDriver<I> {
    type Observation;

    async fn execute(&mut self, interaction: &I) -> Result<Self::Observation, Error>;
}

pub trait Properties<I, O> {
    fn observe(&mut self, interaction: &I, observation: &O) -> Result<(), Error>;
}

pub type TestSuiteParts<S> = (
    <S as TestSuite>::Interactions,
    <S as TestSuite>::Target,
    <S as TestSuite>::Properties,
);

#[async_trait(?Send)]
pub trait TestSuite {
    type Rng;
    type Interaction: std::fmt::Debug;
    type Interactions: Iterator<Item = Self::Interaction> + std::fmt::Debug;
    type Target: TargetDriver<Self::Interaction>;
    type Properties: Properties<Self::Interaction, <Self::Target as TargetDriver<Self::Interaction>>::Observation>;

    async fn build(&self, rng: Self::Rng) -> Result<TestSuiteParts<Self>, Error>
    where
        Self: Sized;

    async fn run(&self, rng: Self::Rng, max_interactions: usize) -> Result<(), Error>
    where
        Self: Sized,
    {
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
