//! Core abstractions for pluggable DST workloads, engines, and properties.

use crate::{config::RunConfig, seed::DstSeed};

/// Pull-based deterministic interaction source.
pub trait NextInteractionSource {
    type Interaction;

    fn next_interaction(&mut self) -> Option<Self::Interaction>;
    fn request_finish(&mut self);
}

/// A workload plan executed on-demand through `next_interaction`.
pub trait WorkloadPlan {
    type Interaction: Clone + Send + Sync + 'static;
    fn next_interactions(
        &self,
        seed: DstSeed,
        cfg: RunConfig,
    ) -> Box<dyn NextInteractionSource<Interaction = Self::Interaction>>;
}

/// Target execution contract over a workload interaction stream.
pub trait TargetEngine<I> {
    type Outcome;
    type Error;

    async fn execute_interaction(&mut self, interaction: &I) -> Result<(), Self::Error>;
    fn finish(&mut self);
    fn collect_outcome(&mut self) -> anyhow::Result<Self::Outcome>;
}

/// Shared streaming runner.
pub async fn run_streaming<I, S, E>(mut source: S, mut engine: E, cfg: RunConfig) -> anyhow::Result<E::Outcome>
where
    I: Clone,
    S: NextInteractionSource<Interaction = I>,
    E: TargetEngine<I, Error = String>,
{
    let deadline = cfg.deadline();
    let mut step = 0usize;
    loop {
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            source.request_finish();
        }
        let Some(interaction) = source.next_interaction() else {
            break;
        };
        engine
            .execute_interaction(&interaction)
            .await
            .map_err(|e| anyhow::anyhow!("interaction execution failed at step {step}: {e}"))?;
        step = step.saturating_add(1);
    }
    engine.finish();
    let outcome = engine.collect_outcome()?;
    Ok(outcome)
}
