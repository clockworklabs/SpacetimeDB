//! Core abstractions for pluggable DST workloads, engines, and properties.

use std::future::Future;

use crate::config::RunConfig;

/// Pull-based deterministic interaction source.
pub trait NextInteractionSource {
    type Interaction;

    fn next_interaction(&mut self) -> Option<Self::Interaction>;
    fn request_finish(&mut self);
}

/// Target execution contract over a workload interaction stream.
pub trait TargetEngine<I> {
    type Observation;
    type Outcome;
    type Error;

    fn execute_interaction<'a>(
        &'a mut self,
        interaction: &'a I,
    ) -> impl Future<Output = Result<Self::Observation, Self::Error>> + 'a;
    fn finish(&mut self);
    fn collect_outcome<'a>(&'a mut self) -> impl Future<Output = anyhow::Result<Self::Outcome>> + 'a;
}

/// Property runtime contract for the shared streaming runner.
pub trait StreamingProperties<I, O, E>
where
    E: TargetEngine<I, Error = String>,
{
    fn observe(&mut self, engine: &E, interaction: &I, observation: &O) -> Result<(), String>;
    fn finish(&mut self, engine: &E, outcome: &E::Outcome) -> Result<(), String>;
}

/// Shared streaming runner with property orchestration.
pub async fn run_streaming<I, S, E, P>(
    mut source: S,
    mut engine: E,
    mut properties: P,
    cfg: RunConfig,
) -> anyhow::Result<E::Outcome>
where
    I: Clone,
    S: NextInteractionSource<Interaction = I>,
    E: TargetEngine<I, Error = String>,
    P: StreamingProperties<I, E::Observation, E>,
{
    // Duration is a harness-level wall-clock stop condition. The reproducible
    // budget for exact replay is `RunConfig::max_interactions`, which the
    // source uses when it is constructed.
    let deadline = cfg.deadline();
    let mut step = 0usize;
    loop {
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            source.request_finish();
        }
        let Some(interaction) = source.next_interaction() else {
            break;
        };
        let observation = engine
            .execute_interaction(&interaction)
            .await
            .map_err(|e| anyhow::anyhow!("interaction execution failed at step {step}: {e}"))?;
        properties
            .observe(&engine, &interaction, &observation)
            .map_err(|e| anyhow::anyhow!("property violation at step {step}: {e}"))?;
        step = step.saturating_add(1);
    }
    engine.finish();
    let outcome = engine.collect_outcome().await?;
    properties
        .finish(&engine, &outcome)
        .map_err(|e| anyhow::anyhow!("property violation at finish: {e}"))?;
    Ok(outcome)
}
