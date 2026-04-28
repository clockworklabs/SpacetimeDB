//! Target descriptor layer used by the CLI.

use crate::{
    config::RunConfig,
    seed::DstSeed,
    workload::{module_ops::HostScenarioId, table_ops::TableScenarioId},
};

/// Descriptor contract: CLI talks to this, not per-target ad hoc handlers.
pub trait TargetDescriptor {
    const NAME: &'static str;
    type Scenario;

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> anyhow::Result<String>;
}

pub struct DatastoreDescriptor;

impl TargetDescriptor for DatastoreDescriptor {
    const NAME: &'static str = "datastore";
    type Scenario = TableScenarioId;

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> anyhow::Result<String> {
        let outcome = crate::targets::datastore::run_generated_with_config_and_scenario(seed, scenario, config)?;
        Ok(format!(
            "ok target={} seed={} tables={} row_counts={:?}",
            Self::NAME,
            seed.0,
            outcome.final_rows.len(),
            outcome.final_row_counts
        ))
    }
}

pub struct RelationalDbCommitlogDescriptor;

impl TargetDescriptor for RelationalDbCommitlogDescriptor {
    const NAME: &'static str = "relational_db_commitlog";
    type Scenario = TableScenarioId;

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> anyhow::Result<String> {
        let outcome =
            crate::targets::relational_db_commitlog::run_generated_with_config_and_scenario(seed, scenario, config)?;
        Ok(format!(
            "ok target={} seed={} steps={} durable_commits={} replay_tables={}",
            Self::NAME,
            seed.0,
            outcome.applied_steps,
            outcome.durable_commit_count,
            outcome.replay_table_count
        ))
    }
}

pub struct StandaloneHostDescriptor;

impl TargetDescriptor for StandaloneHostDescriptor {
    const NAME: &'static str = "standalone_host";
    type Scenario = HostScenarioId;

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> anyhow::Result<String> {
        let outcome = crate::targets::standalone_host::run_generated_with_config_and_scenario(seed, scenario, config)?;
        Ok(format!(
            "ok target={} seed={} steps={} reducer_calls={} waits={} reopens={} noops={} expected_errors={}",
            Self::NAME,
            seed.0,
            outcome.steps_executed,
            outcome.reducer_calls,
            outcome.scheduler_waits,
            outcome.reopens,
            outcome.noops,
            outcome.expected_errors
        ))
    }
}
