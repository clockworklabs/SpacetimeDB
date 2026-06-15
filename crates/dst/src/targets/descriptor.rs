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
            let scenario_name = scenario.as_str();
            let max_interactions = config.max_interactions;
            let duration_ms = config.max_duration_ms;
            let profile = config.commitlog_fault_profile;
            let harness_phase_timeout_ms = config.harness_phase_timeout_ms;
            let outcome =
                crate::targets::relational_db_commitlog::run_generated_with_config_and_scenario(seed, scenario, config)
                    .await?;
            Ok(format!(
                "ok target={} scenario={} seed={} max_interactions={} duration_ms={} harness_phase_timeout_ms={} commitlog_fault_profile={} interactions={} final_row_count={} commitlog_faults={:?}",
                Self::NAME,
                scenario_name,
                seed,
                max_interactions
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                duration_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                harness_phase_timeout_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "off".to_string()),
                profile,
                outcome.interactions_executed,
                outcome.final_row_counts.iter().sum::<u64>(),
                outcome.commitlog_fault_summary,
            ))
        })
    }
}
