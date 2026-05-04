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
            Ok(format_relational_db_commitlog_outcome(Self::NAME, seed, &outcome))
        })
    }
}

fn format_relational_db_commitlog_outcome(
    target: &str,
    seed: DstSeed,
    outcome: &crate::targets::relational_db_commitlog::RelationalDbCommitlogOutcome,
) -> String {
    let alive_tasks = outcome
        .runtime
        .runtime_alive_tasks
        .map(|count| count.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        concat!(
            "ok target={} seed={} steps={}\n",
            "\n",
            "schema: tables={} columns={} max_columns={} indexes={} extra_indexes={}\n",
            "durability: durable_commits={} replay_tables={}\n",
            "interactions: table={} creates={} drops={} migrates={} reopens={} reopen_skipped={} skipped={}\n",
            "table_ops:\n",
            "  tx_control: begin={} commit={} rollback={} begin_read={} release_read={} begin_conflict={} write_conflict={}\n",
            "  writes: insert={} delete={} exact_dup={} unique_conflict={} missing_delete={} batch_insert={} batch_delete={} reinsert={}\n",
            "  schema: add_column={} add_index={}\n",
            "  reads: point_lookup={} predicate_count={} range_scan={} full_scan={}\n",
            "transactions: begin={} commit={} rollback={} auto_commit={} read_tx={}\n",
            "disk_faults: profile={} latency={} short_read={} short_write={} errors(read={} write={} flush={} fsync={} open={} metadata={})\n",
            "runtime: known_tasks={} durability_actors={} alive_tasks={}"
        ),
        target,
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
        outcome.interactions.close_reopen_applied,
        outcome.interactions.close_reopen_skipped,
        outcome.interactions.skipped,
        outcome.table_ops.begin_tx,
        outcome.table_ops.commit_tx,
        outcome.table_ops.rollback_tx,
        outcome.table_ops.begin_read_tx,
        outcome.table_ops.release_read_tx,
        outcome.table_ops.begin_tx_conflict,
        outcome.table_ops.write_conflict_insert,
        outcome.table_ops.insert,
        outcome.table_ops.delete,
        outcome.table_ops.exact_duplicate_insert,
        outcome.table_ops.unique_key_conflict_insert,
        outcome.table_ops.delete_missing,
        outcome.table_ops.batch_insert,
        outcome.table_ops.batch_delete,
        outcome.table_ops.reinsert,
        outcome.table_ops.add_column,
        outcome.table_ops.add_index,
        outcome.table_ops.point_lookup,
        outcome.table_ops.predicate_count,
        outcome.table_ops.range_scan,
        outcome.table_ops.full_scan,
        outcome.transactions.explicit_begin,
        outcome.transactions.explicit_commit,
        outcome.transactions.explicit_rollback,
        outcome.transactions.auto_commit,
        outcome.transactions.read_tx,
        outcome.disk_faults.profile,
        outcome.disk_faults.latency,
        outcome.disk_faults.short_read,
        outcome.disk_faults.short_write,
        outcome.disk_faults.read_error,
        outcome.disk_faults.write_error,
        outcome.disk_faults.flush_error,
        outcome.disk_faults.fsync_error,
        outcome.disk_faults.open_error,
        outcome.disk_faults.metadata_error,
        outcome.runtime.known_tokio_tasks_scheduled,
        outcome.runtime.durability_actors_started,
        alive_tasks
    )
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
