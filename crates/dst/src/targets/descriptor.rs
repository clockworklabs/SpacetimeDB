//! Target descriptor layer used by the CLI.

use std::{future::Future, pin::Pin};

use crate::{config::RunConfig, seed::DstSeed};

/// Descriptor contract: CLI talks to this, not per-target ad hoc handlers.
pub trait TargetDescriptor {
    const NAME: &'static str;
    type Scenario;

    fn prepare(_seed: DstSeed, _scenario: &Self::Scenario, _config: &RunConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> TargetRunFuture;
}

pub type TargetRunFuture = Pin<Box<dyn Future<Output = anyhow::Result<String>>>>;

pub struct RelationalDbConcurrentDescriptor;

impl TargetDescriptor for RelationalDbConcurrentDescriptor {
    const NAME: &'static str = "relational_db_concurrent";
    type Scenario = ();

    fn run_streaming(seed: DstSeed, _scenario: Self::Scenario, config: RunConfig) -> TargetRunFuture {
        Box::pin(async move {
            let outcome = crate::targets::relational_db_concurrent::run_generated_with_config(seed, config).await?;
            Ok(format_relational_db_concurrent_outcome(Self::NAME, seed, &outcome))
        })
    }
}

fn format_relational_db_concurrent_outcome(
    target: &str,
    seed: DstSeed,
    outcome: &crate::targets::relational_db_concurrent::RelationalDbConcurrentOutcome,
) -> String {
    format!(
        concat!(
            "ok target={} seed={} rounds={}\n",
            "\n",
            "clients={} events={} reads={}\n",
            "transactions: committed={} write_conflicts={} writer_conflicts={} reader_conflicts={}\n",
            "rows: final={} expected={}"
        ),
        target,
        seed.0,
        outcome.rounds,
        outcome.clients,
        outcome.events,
        outcome.reads,
        outcome.committed,
        outcome.write_conflicts,
        outcome.writer_conflicts,
        outcome.reader_conflicts,
        outcome.final_rows.len(),
        outcome.expected_rows.len(),
    )
}
