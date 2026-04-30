//! Target descriptor layer used by the CLI.

use std::{future::Future, pin::Pin};

use crate::{
    config::RunConfig,
    seed::DstSeed,
    workload::{module_ops::HostScenarioId, table_ops::TableScenarioId},
};

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

pub struct RelationalDbCommitlogDescriptor;

impl TargetDescriptor for RelationalDbCommitlogDescriptor {
    const NAME: &'static str = "relational_db_commitlog";
    type Scenario = TableScenarioId;

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> TargetRunFuture {
        Box::pin(async move {
            let outcome =
                crate::targets::relational_db_commitlog::run_generated_with_config_and_scenario(seed, scenario, config)
                    .await?;
            let alive_tasks = outcome
                .runtime
                .runtime_alive_tasks
                .map(|count| count.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Ok(format!(
                "ok target={} seed={} steps={} schema_tables={} schema_columns={} schema_max_columns={} schema_indexes={} schema_extra_indexes={} durable_commits={} replay_tables={} table_ops={} creates={} drops={} migrates={} syncs={} reopens={} reopen_skipped={} skipped={} op_begin={} op_commit={} op_rollback={} op_insert={} op_delete={} op_dup_insert={} op_missing_delete={} op_batch_insert={} op_batch_delete={} op_reinsert={} op_point_lookup={} op_predicate_count={} op_range_scan={} op_full_scan={} tx_begin={} tx_commit={} tx_rollback={} auto_commit={} read_tx={} known_tasks={} durability_actors={} alive_tasks={}",
                Self::NAME,
                seed.0,
                outcome.applied_steps,
                outcome.schema.initial_tables,
                outcome.schema.initial_columns,
                outcome.schema.max_columns_per_table,
                outcome.schema.initial_indexes,
                outcome.schema.extra_indexes,
                outcome.durable_commit_count,
                outcome.replay_table_count,
                outcome.interactions.table,
                outcome.interactions.create_dynamic_table,
                outcome.interactions.drop_dynamic_table,
                outcome.interactions.migrate_dynamic_table,
                outcome.interactions.chaos_sync,
                outcome.interactions.close_reopen_applied,
                outcome.interactions.close_reopen_skipped,
                outcome.interactions.skipped,
                outcome.table_ops.begin_tx,
                outcome.table_ops.commit_tx,
                outcome.table_ops.rollback_tx,
                outcome.table_ops.insert,
                outcome.table_ops.delete,
                outcome.table_ops.duplicate_insert,
                outcome.table_ops.delete_missing,
                outcome.table_ops.batch_insert,
                outcome.table_ops.batch_delete,
                outcome.table_ops.reinsert,
                outcome.table_ops.point_lookup,
                outcome.table_ops.predicate_count,
                outcome.table_ops.range_scan,
                outcome.table_ops.full_scan,
                outcome.transactions.explicit_begin,
                outcome.transactions.explicit_commit,
                outcome.transactions.explicit_rollback,
                outcome.transactions.auto_commit,
                outcome.transactions.read_tx,
                outcome.runtime.known_tokio_tasks_scheduled,
                outcome.runtime.durability_actors_started,
                alive_tasks
            ))
        })
    }
}

pub struct StandaloneHostDescriptor;

impl TargetDescriptor for StandaloneHostDescriptor {
    const NAME: &'static str = "standalone_host";
    type Scenario = HostScenarioId;

    fn prepare(_seed: DstSeed, _scenario: &Self::Scenario, _config: &RunConfig) -> anyhow::Result<()> {
        crate::targets::standalone_host::prepare_generated_run()
    }

    fn run_streaming(seed: DstSeed, scenario: Self::Scenario, config: RunConfig) -> TargetRunFuture {
        Box::pin(async move {
            let outcome =
                crate::targets::standalone_host::run_generated_with_config_and_scenario(seed, scenario, config).await?;
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
        })
    }
}
