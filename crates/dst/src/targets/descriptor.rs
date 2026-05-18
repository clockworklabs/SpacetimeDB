//! Target descriptor layer used by the CLI.

use std::{future::Future, pin::Pin};

use crate::{config::RunConfig, workload::table_ops::TableScenarioId};

/// Descriptor contract: CLI talks to this, not per-target ad hoc handlers.
pub trait TargetDescriptor {
    const NAME: &'static str;
    type Scenario;

    fn prepare(_seed: u64, _scenario: &Self::Scenario, _config: &RunConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn run_streaming(seed: u64, scenario: Self::Scenario, config: RunConfig) -> TargetRunFuture;
}

pub type TargetRunFuture = Pin<Box<dyn Future<Output = anyhow::Result<String>>>>;

pub struct RelationalDbCommitlogDescriptor;

impl TargetDescriptor for RelationalDbCommitlogDescriptor {
    const NAME: &'static str = "relational-db-commitlog";
    type Scenario = TableScenarioId;

    fn run_streaming(seed: u64, scenario: Self::Scenario, config: RunConfig) -> TargetRunFuture {
        Box::pin(async move {
            let outcome =
                crate::targets::relational_db_commitlog::run_generated_with_config_and_scenario(seed, scenario, config)
                    .await?;
            Ok(format!(
                "ok target={} seed={} steps={}",
                Self::NAME,
                seed,
                outcome.final_row_counts.iter().sum::<u64>(),
            ))
        })
    }
}
