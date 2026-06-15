//! Core abstractions for pluggable DST workloads, engines, and properties.

use std::{
    any::Any,
    fmt::Debug,
    future::Future,
    panic::{self, AssertUnwindSafe},
    time::Duration,
};

use crate::config::RunConfig;
use futures_util::FutureExt;

/// Pull-based deterministic interaction source.
pub trait WorkloadSource {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RunStats {
    pub interactions_executed: usize,
}

pub trait RunOutcome {
    fn record_run_stats(&mut self, stats: RunStats);
}

impl RunOutcome for () {
    fn record_run_stats(&mut self, _stats: RunStats) {}
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
    I: Clone + Debug,
    S: WorkloadSource<Interaction = I>,
    E: TargetEngine<I, Error = String>,
    E::Outcome: RunOutcome,
    P: StreamingProperties<I, E::Observation, E>,
{
    let deadline = cfg.deadline();
    let phase_timeout = cfg.harness_phase_timeout_ms.map(Duration::from_millis);
    let mut step = 0usize;
    loop {
        if cfg.max_interactions.is_some_and(|max| step >= max) {
            break;
        }
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            source.request_finish();
        }
        let Some(interaction) = source.next_interaction() else {
            break;
        };
        let execution = guard_target("execute_interaction", step, Some(&interaction), phase_timeout, || {
            engine.execute_interaction(&interaction)
        })
        .await
        .map_err(|e| anyhow::anyhow!("property violation at step {step}: {e}"))?;
        let observation = execution.map_err(|e| anyhow::anyhow!("interaction execution failed at step {step}: {e}"))?;
        let property_result = guard_sync("properties.observe", step, Some(&interaction), || {
            properties.observe(&engine, &interaction, &observation)
        })
        .map_err(|e| anyhow::anyhow!("property violation at step {step}: {e}"))?;
        property_result.map_err(|e| anyhow::anyhow!("property violation at step {step}: {e}"))?;
        step = step.saturating_add(1);
    }
    guard_target("finish", step, Option::<&I>::None, phase_timeout, || async {
        engine.finish();
    })
    .await
    .map_err(|e| anyhow::anyhow!("property violation at finish: {e}"))?;
    let mut outcome = guard_target("collect_outcome", step, Option::<&I>::None, phase_timeout, || {
        engine.collect_outcome()
    })
    .await
    .map_err(|e| anyhow::anyhow!("property violation while collecting outcome: {e}"))??;
    outcome.record_run_stats(RunStats {
        interactions_executed: step,
    });
    let property_result = guard_sync("properties.finish", step, Option::<&I>::None, || {
        properties.finish(&engine, &outcome)
    })
    .map_err(|e| anyhow::anyhow!("property violation at finish: {e}"))?;
    property_result.map_err(|e| anyhow::anyhow!("property violation at finish: {e}"))?;
    Ok(outcome)
}

async fn guard_target<T, Fut, I>(
    phase: &'static str,
    step: usize,
    interaction: Option<&I>,
    timeout: Option<Duration>,
    make_future: impl FnOnce() -> Fut,
) -> Result<T, String>
where
    I: Debug,
    Fut: Future<Output = T>,
{
    let future = panic::catch_unwind(AssertUnwindSafe(make_future))
        .map_err(|payload| not_crash_error(phase, step, interaction, &payload))?;
    let guarded = AssertUnwindSafe(future).catch_unwind();

    match timeout {
        Some(timeout) => match crate::sim::time::timeout(timeout, guarded).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(payload)) => Err(not_crash_error(phase, step, interaction, &payload)),
            Err(elapsed) => Err(timeout_error(phase, step, interaction, elapsed.duration())),
        },
        None => guarded
            .await
            .map_err(|payload| not_crash_error(phase, step, interaction, &payload)),
    }
}

fn guard_sync<T, I>(
    phase: &'static str,
    step: usize,
    interaction: Option<&I>,
    f: impl FnOnce() -> T,
) -> Result<T, String>
where
    I: Debug,
{
    panic::catch_unwind(AssertUnwindSafe(f)).map_err(|payload| not_crash_error(phase, step, interaction, &payload))
}

fn timeout_error<I: Debug>(phase: &'static str, step: usize, interaction: Option<&I>, timeout: Duration) -> String {
    match interaction {
        Some(interaction) => format!(
            "[Timeout] target did not complete {phase} within {timeout:?} at step {step}: interaction={interaction:?}"
        ),
        None => format!("[Timeout] target did not complete {phase} within {timeout:?} after step {step}"),
    }
}

fn not_crash_error<I: Debug>(
    phase: &'static str,
    step: usize,
    interaction: Option<&I>,
    payload: &Box<dyn Any + Send>,
) -> String {
    let payload = panic_payload_to_string(payload);
    match interaction {
        Some(interaction) => {
            format!("[NotCrash] target panicked during {phase} at step {step}: interaction={interaction:?}, payload={payload}")
        }
        None => format!("[NotCrash] target panicked during {phase} after step {step}: payload={payload}"),
    }
}

fn panic_payload_to_string(payload: &Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestInteraction;

    struct SingleStepSource {
        emitted: bool,
    }

    impl SingleStepSource {
        fn new() -> Self {
            Self { emitted: false }
        }
    }

    impl WorkloadSource for SingleStepSource {
        type Interaction = TestInteraction;

        fn next_interaction(&mut self) -> Option<Self::Interaction> {
            if self.emitted {
                None
            } else {
                self.emitted = true;
                Some(TestInteraction)
            }
        }

        fn request_finish(&mut self) {}
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum PanicPhase {
        Execute,
        Finish,
        CollectOutcome,
    }

    struct PanicEngine {
        phase: PanicPhase,
    }

    struct PendingEngine;

    impl PanicEngine {
        fn new(phase: PanicPhase) -> Self {
            Self { phase }
        }
    }

    #[allow(clippy::manual_async_fn)]
    impl TargetEngine<TestInteraction> for PanicEngine {
        type Observation = ();
        type Outcome = ();
        type Error = String;

        fn execute_interaction<'a>(
            &'a mut self,
            _interaction: &'a TestInteraction,
        ) -> impl Future<Output = Result<Self::Observation, Self::Error>> + 'a {
            async move {
                if self.phase == PanicPhase::Execute {
                    panic!("execute panic");
                }
                Ok(())
            }
        }

        fn finish(&mut self) {
            if self.phase == PanicPhase::Finish {
                panic!("finish panic");
            }
        }

        fn collect_outcome<'a>(&'a mut self) -> impl Future<Output = anyhow::Result<Self::Outcome>> + 'a {
            async move {
                if self.phase == PanicPhase::CollectOutcome {
                    panic!("collect panic");
                }
                Ok(())
            }
        }
    }

    #[allow(clippy::manual_async_fn)]
    impl TargetEngine<TestInteraction> for PendingEngine {
        type Observation = ();
        type Outcome = ();
        type Error = String;

        fn execute_interaction<'a>(
            &'a mut self,
            _interaction: &'a TestInteraction,
        ) -> impl Future<Output = Result<Self::Observation, Self::Error>> + 'a {
            futures_util::future::pending()
        }

        fn finish(&mut self) {}

        fn collect_outcome<'a>(&'a mut self) -> impl Future<Output = anyhow::Result<Self::Outcome>> + 'a {
            async move { Ok(()) }
        }
    }

    struct NoopProperties;

    impl<E> StreamingProperties<TestInteraction, (), E> for NoopProperties
    where
        E: TargetEngine<TestInteraction, Observation = (), Outcome = (), Error = String>,
    {
        fn observe(&mut self, _engine: &E, _interaction: &TestInteraction, _observation: &()) -> Result<(), String> {
            Ok(())
        }

        fn finish(&mut self, _engine: &E, _outcome: &()) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn not_crash_catches_execute_panic() {
        assert_not_crash_error(PanicPhase::Execute, "execute_interaction", "execute panic");
    }

    #[test]
    fn not_crash_catches_finish_panic() {
        assert_not_crash_error(PanicPhase::Finish, "finish", "finish panic");
    }

    #[test]
    fn not_crash_catches_collect_outcome_panic() {
        assert_not_crash_error(PanicPhase::CollectOutcome, "collect_outcome", "collect panic");
    }

    #[test]
    fn target_timeout_reports_stalled_interaction() {
        let mut runtime = crate::sim::Runtime::new(0).expect("runtime");
        let err = runtime
            .block_on(run_streaming(
                SingleStepSource::new(),
                PendingEngine,
                NoopProperties,
                RunConfig {
                    max_interactions: Some(1),
                    max_duration_ms: None,
                    harness_phase_timeout_ms: Some(1),
                    commitlog_fault_profile: crate::config::CommitlogFaultProfile::Off,
                },
            ))
            .unwrap_err()
            .to_string();

        assert!(err.contains("[Timeout]"));
        assert!(err.contains("execute_interaction"));
    }

    fn assert_not_crash_error(phase: PanicPhase, expected_phase: &str, expected_payload: &str) {
        let mut runtime = crate::sim::Runtime::new(0).expect("runtime");
        let err = runtime
            .block_on(run_streaming(
                SingleStepSource::new(),
                PanicEngine::new(phase),
                NoopProperties,
                RunConfig::with_max_interactions(1),
            ))
            .unwrap_err()
            .to_string();

        assert!(err.contains("[NotCrash]"));
        assert!(err.contains(expected_phase));
        assert!(err.contains(expected_payload));
    }
}
