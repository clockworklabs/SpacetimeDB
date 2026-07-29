use std::{
    fmt::Debug,
    panic::{resume_unwind, AssertUnwindSafe},
};

use anyhow::{Context, Error};
use futures::FutureExt;
use spacetimedb_runtime::sim::Rng;

const PROGRESS_INTERVAL: usize = 10_000;

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

/// Stateful source of interactions that can incorporate target feedback.
pub trait InteractionGen {
    type Interaction: Debug;
    type Observation;

    fn next_interaction(&mut self) -> Option<Self::Interaction>;

    fn observe(&mut self, interaction: &Self::Interaction, observation: &Self::Observation) -> Result<(), Error>;
}

pub type TestSuiteParts<S> = (
    <S as TestSuite>::Interactions,
    <S as TestSuite>::Target,
    <S as TestSuite>::Properties,
);

pub trait TestSuite {
    type Interaction: Debug;
    type Interactions: InteractionGen<
            Interaction = Self::Interaction,
            Observation = <Self::Target as TargetDriver<Self::Interaction>>::Observation,
        > + Debug;
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

            let result = AssertUnwindSafe(async {
                for step in 0..max_interactions {
                    let Some(interaction) = interactions.next_interaction() else {
                        break;
                    };

                    let observation = target
                        .execute(&interaction)
                        .await
                        .with_context(|| format!("DST target failed at interaction #{step}: {interaction:?}"))?;

                    properties
                        .observe(&interaction, &observation)
                        .with_context(|| format!("DST property failed at interaction #{step}: {interaction:?}"))?;

                    interactions.observe(&interaction, &observation).with_context(|| {
                        format!("DST generator feedback failed at interaction #{step}: {interaction:?}")
                    })?;

                    let completed = step + 1;
                    if completed % PROGRESS_INTERVAL == 0 {
                        eprintln!("interaction progress: completed {completed} interactions: {interactions:?}");
                    }
                }

                Ok(())
            })
            .catch_unwind()
            .await;

            eprintln!("final interaction counts: {interactions:?}");
            tracing::info!(interaction_counts = ?interactions, "final interaction counts");

            match result {
                Ok(result) => result,
                Err(payload) => resume_unwind(payload),
            }
        }
    }
}
