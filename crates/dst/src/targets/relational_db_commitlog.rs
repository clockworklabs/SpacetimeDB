//! RelationalDB DST target with mocked commitlog file chaos and replay checks.

use std::{cell::Cell, collections::BTreeMap, io, num::NonZeroU64, ops::Bound, panic::AssertUnwindSafe, sync::Arc};

use spacetimedb_commitlog::repo::{Memory as MemoryCommitlogRepo, SizeOnDisk};
use spacetimedb_core::{
    db::relational_db::{MutTx as RelMutTx, Persistence, RelationalDB, Tx as RelTx},
    error::{DBError, DatastoreError, IndexError},
    messages::control_db::HostType,
};
use spacetimedb_datastore::{
    execution_context::Workload,
    traits::{IsolationLevel, Program},
};
use spacetimedb_durability::{Durability, EmptyHistory, Local};
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::{SequenceId, TableId};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;
use tracing::{debug, info, trace};

use crate::{
    config::{CommitlogFaultProfile, RunConfig},
    core::{self, TargetEngine},
    schema::{SchemaPlan, SimRow},
    seed::DstSeed,
    targets::buggified_repo::{is_injected_disk_error_text, BuggifiedRepo, CommitlogFaultConfig},
    targets::properties::{
        CommitlogObservation, DynamicMigrationProbe, PropertyRuntime, TableObservation, TargetPropertyAccess,
    },
    workload::{
        commitlog_ops::{CommitlogInteraction, CommitlogWorkloadOutcome, DurableReplaySummary},
        commitlog_ops::{InteractionSummary, RuntimeSummary, SchemaSummary, TableOperationSummary, TransactionSummary},
        table_ops::{
            ConnectionWriteState, ExpectedErrorKind, TableOperation, TableScenario, TableScenarioId,
            TableWorkloadInteraction, TableWorkloadOutcome,
        },
    },
};

pub type RelationalDbCommitlogOutcome = CommitlogWorkloadOutcome;
type RelationalDbCommitlogSource = crate::workload::commitlog_ops::CommitlogWorkloadSource<TableScenarioId>;
type RelationalDbCommitlogProperties = PropertyRuntime;

pub async fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<RelationalDbCommitlogOutcome> {
    let (source, engine, properties) = build(seed, scenario, &config)?;
    let outcome = core::run_streaming(source, engine, properties, config).await?;
    info!(
        applied_steps = outcome.applied_steps,
        durable_commit_count = outcome.durable_commit_count,
        replay_table_count = outcome.replay_table_count,
        "relational_db_commitlog complete"
    );
    Ok(outcome)
}

fn build(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: &RunConfig,
) -> anyhow::Result<(
    RelationalDbCommitlogSource,
    RelationalDbEngine,
    RelationalDbCommitlogProperties,
)> {
    let mut connection_rng = seed.fork(121).rng();
    let num_connections = connection_rng.index(3) + 1;
    let mut schema_rng = seed.fork(122).rng();
    let schema = scenario.generate_schema(&mut schema_rng);
    let generator = crate::workload::commitlog_ops::CommitlogWorkloadSource::new(
        seed,
        scenario,
        schema.clone(),
        num_connections,
        config.max_interactions_or_default(usize::MAX),
    );
    let engine = RelationalDbEngine::new(seed, &schema, num_connections, config.commitlog_fault_profile)?;
    let properties = PropertyRuntime::for_table_workload(scenario, schema.clone(), num_connections);
    Ok((generator, engine, properties))
}

#[derive(Clone, Debug)]
struct DynamicTableState {
    name: String,
    version: u32,
    table_id: TableId,
}

#[derive(Default)]
struct RunStats {
    interactions: InteractionSummary,
    table_ops: TableOperationSummary,
    transactions: TransactionStats,
    runtime: RuntimeStats,
}

#[derive(Default)]
struct TransactionStats {
    explicit_begin: usize,
    explicit_commit: usize,
    explicit_rollback: usize,
    auto_commit: usize,
    read_tx: Cell<usize>,
}

#[derive(Default)]
struct RuntimeStats {
    durability_actors_started: usize,
}

impl RunStats {
    fn record_interaction_requested(&mut self, interaction: &CommitlogInteraction) {
        match interaction {
            CommitlogInteraction::Table(_) => self.interactions.table += 1,
            CommitlogInteraction::CreateDynamicTable { .. } => self.interactions.create_dynamic_table += 1,
            CommitlogInteraction::DropDynamicTable { .. } => self.interactions.drop_dynamic_table += 1,
            CommitlogInteraction::MigrateDynamicTable { .. } => self.interactions.migrate_dynamic_table += 1,
            CommitlogInteraction::ChaosSync => self.interactions.chaos_sync += 1,
            CommitlogInteraction::CloseReopen => self.interactions.close_reopen_requested += 1,
        }
    }

    fn record_interaction_result(&mut self, interaction: &CommitlogInteraction, observation: &CommitlogObservation) {
        if matches!(observation, CommitlogObservation::Skipped) {
            self.interactions.skipped += 1;
        }
        if matches!(interaction, CommitlogInteraction::CloseReopen) {
            match observation {
                CommitlogObservation::Skipped => self.interactions.close_reopen_skipped += 1,
                CommitlogObservation::Applied | CommitlogObservation::DurableReplay(_) => {
                    self.interactions.close_reopen_applied += 1
                }
                _ => {}
            }
        }
    }

    fn record_table_operation(&mut self, op: &TableOperation) {
        match op {
            TableOperation::BeginTx { .. } => self.table_ops.begin_tx += 1,
            TableOperation::CommitTx { .. } => self.table_ops.commit_tx += 1,
            TableOperation::RollbackTx { .. } => self.table_ops.rollback_tx += 1,
            TableOperation::BeginReadTx { .. } => self.table_ops.begin_read_tx += 1,
            TableOperation::ReleaseReadTx { .. } => self.table_ops.release_read_tx += 1,
            TableOperation::BeginTxConflict { .. } => self.table_ops.begin_tx_conflict += 1,
            TableOperation::WriteConflictInsert { .. } => self.table_ops.write_conflict_insert += 1,
            TableOperation::Insert { .. } => self.table_ops.insert += 1,
            TableOperation::Delete { .. } => self.table_ops.delete += 1,
            TableOperation::ExactDuplicateInsert { .. } => self.table_ops.exact_duplicate_insert += 1,
            TableOperation::UniqueKeyConflictInsert { .. } => self.table_ops.unique_key_conflict_insert += 1,
            TableOperation::DeleteMissing { .. } => self.table_ops.delete_missing += 1,
            TableOperation::BatchInsert { .. } => self.table_ops.batch_insert += 1,
            TableOperation::BatchDelete { .. } => self.table_ops.batch_delete += 1,
            TableOperation::Reinsert { .. } => self.table_ops.reinsert += 1,
            TableOperation::AddColumn { .. } => self.table_ops.add_column += 1,
            TableOperation::AddIndex { .. } => self.table_ops.add_index += 1,
            TableOperation::PointLookup { .. } => self.table_ops.point_lookup += 1,
            TableOperation::PredicateCount { .. } => self.table_ops.predicate_count += 1,
            TableOperation::RangeScan { .. } => self.table_ops.range_scan += 1,
            TableOperation::FullScan { .. } => self.table_ops.full_scan += 1,
        }
    }

    fn record_read_tx(&self) {
        self.transactions
            .read_tx
            .set(self.transactions.read_tx.get().saturating_add(1));
    }

    fn transaction_summary(&self, durable_commit_count: usize) -> TransactionSummary {
        TransactionSummary {
            explicit_begin: self.transactions.explicit_begin,
            explicit_commit: self.transactions.explicit_commit,
            explicit_rollback: self.transactions.explicit_rollback,
            auto_commit: self.transactions.auto_commit,
            read_tx: self.transactions.read_tx.get(),
            durable_commit_count,
        }
    }

    fn runtime_summary(&self) -> RuntimeSummary {
        RuntimeSummary {
            known_tokio_tasks_scheduled: self.runtime.durability_actors_started,
            durability_actors_started: self.runtime.durability_actors_started,
            runtime_alive_tasks: runtime_alive_tasks(),
        }
    }
}

/// Engine executing mixed table+lifecycle interactions while recording mocked durable history.
struct RelationalDbEngine {
    db: Option<RelationalDB>,
    execution: ConnectionWriteState<RelMutTx>,
    read_tx_by_connection: Vec<Option<RelTx>>,
    base_schema: SchemaPlan,
    base_table_ids: Vec<TableId>,
    dynamic_tables: BTreeMap<u32, DynamicTableState>,
    step: usize,
    last_requested_durable_offset: Option<u64>,
    last_observed_durable_offset: Option<u64>,
    durability: Arc<InMemoryCommitlogDurability>,
    durability_opts: spacetimedb_durability::local::Options,
    runtime_handle: tokio::runtime::Handle,
    commitlog_repo: StressCommitlogRepo,
    stats: RunStats,
    _runtime_guard: Option<tokio::runtime::Runtime>,
}

impl RelationalDbEngine {
    fn new(
        seed: DstSeed,
        schema: &SchemaPlan,
        num_connections: usize,
        fault_profile: CommitlogFaultProfile,
    ) -> anyhow::Result<Self> {
        let bootstrap = bootstrap_relational_db(seed.fork(700), fault_profile)?;
        let mut this = Self {
            db: Some(bootstrap.db),
            execution: ConnectionWriteState::new(num_connections),
            read_tx_by_connection: (0..num_connections).map(|_| None).collect(),
            base_schema: schema.clone(),
            base_table_ids: Vec::with_capacity(schema.tables.len()),
            dynamic_tables: BTreeMap::new(),
            step: 0,
            last_requested_durable_offset: None,
            last_observed_durable_offset: None,
            durability: bootstrap.durability,
            durability_opts: bootstrap.durability_opts,
            runtime_handle: bootstrap.runtime_handle,
            commitlog_repo: bootstrap.commitlog_repo,
            stats: RunStats {
                runtime: RuntimeStats {
                    durability_actors_started: 1,
                },
                ..Default::default()
            },
            _runtime_guard: bootstrap.runtime_guard,
        };
        this.install_base_schema().map_err(anyhow::Error::msg)?;
        this.refresh_observed_durable_offset(true).map_err(anyhow::Error::msg)?;
        this.commitlog_repo.enable_faults();
        Ok(this)
    }

    fn install_base_schema(&mut self) -> Result<(), String> {
        let mut tx = self
            .db()?
            .begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        for table in &self.base_schema.tables {
            let columns = table
                .columns
                .iter()
                .enumerate()
                .map(|(idx, col)| ColumnSchema::for_test(idx as u16, &col.name, col.ty.clone()))
                .collect::<Vec<_>>();
            let mut indexes = vec![IndexSchema::for_test(
                format!("{}_id_idx", table.name),
                BTreeAlgorithm::from(0),
            )];
            for cols in &table.extra_indexes {
                let cols_name = cols.iter().map(|col| format!("c{col}")).collect::<Vec<_>>().join("_");
                indexes.push(IndexSchema::for_test(
                    format!("{}_{}_idx", table.name, cols_name),
                    BTreeAlgorithm::from(cols.iter().copied().collect::<spacetimedb_primitives::ColList>()),
                ));
            }
            let constraints = vec![ConstraintSchema::unique_for_test(
                format!("{}_id_unique", table.name),
                0,
            )];
            let table_id = self
                .db()?
                .create_table(
                    &mut tx,
                    TableSchema::new(
                        TableId::SENTINEL,
                        TableName::for_test(&table.name),
                        None,
                        columns,
                        indexes,
                        constraints,
                        vec![],
                        StTableType::User,
                        StAccess::Public,
                        None,
                        Some(0.into()),
                        false,
                        None,
                    ),
                )
                .map_err(|err| format!("create table '{}' failed: {err}", table.name))?;
            self.base_table_ids.push(table_id);
        }
        let committed = self
            .db()?
            .commit_tx(tx)
            .map_err(|err| format!("install base schema commit failed: {err}"))?;
        self.record_committed_offset(committed.as_ref().map(|(tx_offset, ..)| *tx_offset));
        Ok(())
    }

    async fn execute(&mut self, interaction: &CommitlogInteraction) -> Result<CommitlogObservation, String> {
        self.step = self.step.saturating_add(1);
        self.stats.record_interaction_requested(interaction);
        let force_sync_after = matches!(interaction, CommitlogInteraction::ChaosSync);
        let observation = match interaction {
            CommitlogInteraction::Table(op) => self.execute_table_op(op).map(CommitlogObservation::Table),
            CommitlogInteraction::CreateDynamicTable { conn, slot } => self.create_dynamic_table(*conn, *slot),
            CommitlogInteraction::DropDynamicTable { conn, slot } => self.drop_dynamic_table(*conn, *slot),
            CommitlogInteraction::MigrateDynamicTable { conn, slot } => self.migrate_dynamic_table(*conn, *slot),
            CommitlogInteraction::ChaosSync => Ok(CommitlogObservation::Applied),
            CommitlogInteraction::CloseReopen => self.close_and_reopen().await,
        }?;
        if !matches!(interaction, CommitlogInteraction::CloseReopen) {
            self.wait_for_requested_durability(force_sync_after).await?;
        }
        self.stats.record_interaction_result(interaction, &observation);
        Ok(observation)
    }

    async fn close_and_reopen(&mut self) -> Result<CommitlogObservation, String> {
        if self.execution.active_writer.is_some()
            || self.execution.tx_by_connection.iter().any(|tx| tx.is_some())
            || self.read_tx_by_connection.iter().any(|tx| tx.is_some())
        {
            trace!("skip close/reopen while transaction is open");
            return Ok(CommitlogObservation::Skipped);
        }

        self.wait_for_requested_durability(true).await?;
        // Explicitly drop the current RelationalDB instance before attempting
        // to open a new durability+DB pair on the same replica directory.
        let old_db = self
            .db
            .take()
            .ok_or_else(|| "close/reopen failed: relational db not initialized".to_string())?;
        old_db.shutdown().await;
        drop(old_db);
        info!("starting in-memory durability");

        let (durability, db) = self.reopen_from_history_with_fault_retry("close/reopen")?;

        self.stats.runtime.durability_actors_started += 1;
        self.durability = durability;
        self.db = Some(db);
        self.rebuild_table_handles_after_reopen()?;
        self.last_observed_durable_offset = self.durability.durable_tx_offset().last_seen();
        let replay = self.durable_replay_summary()?;
        debug!(
            base_tables = self.base_table_ids.len(),
            dynamic_tables = self.dynamic_tables.len(),
            "reopened relational db from durable history"
        );
        Ok(CommitlogObservation::DurableReplay(replay))
    }

    fn reopen_from_history_with_fault_retry(
        &self,
        context: &'static str,
    ) -> Result<(Arc<InMemoryCommitlogDurability>, RelationalDB), String> {
        match self.reopen_from_history() {
            Ok(reopened) => Ok(reopened),
            Err(err) if is_injected_disk_error_text(&err) => {
                trace!(error = %err, "retrying {context} with injected disk faults suspended");
                self.commitlog_repo.with_faults_suspended(|| self.reopen_from_history())
            }
            Err(err) => Err(err),
        }
    }

    fn reopen_from_history(&self) -> Result<(Arc<InMemoryCommitlogDurability>, RelationalDB), String> {
        let durability = Arc::new(
            InMemoryCommitlogDurability::open_with_repo(
                self.commitlog_repo.clone(),
                self.runtime_handle.clone(),
                self.durability_opts,
            )
            .map_err(|err| format!("reopen in-memory durability failed: {err}"))?,
        );
        let persistence = Persistence {
            durability: durability.clone(),
            disk_size: Arc::new(in_memory_size_on_disk),
            snapshots: None,
            runtime: self.runtime_handle.clone(),
        };
        let (db, connected_clients) = RelationalDB::open(
            Identity::ZERO,
            Identity::ZERO,
            durability.as_history(),
            Some(persistence),
            None,
            PagePool::new_for_test(),
        )
        .map_err(|err| format!("close/reopen failed: {err}"))?;
        if !connected_clients.is_empty() {
            return Err(format!(
                "unexpected connected clients after reopen: {connected_clients:?}"
            ));
        }
        Ok((durability, db))
    }

    fn rebuild_table_handles_after_reopen(&mut self) -> Result<(), String> {
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
        self.stats.record_read_tx();
        let schemas = db
            .get_all_tables(&tx)
            .map_err(|err| format!("list tables after reopen failed: {err}"))?;
        let _ = db.release_tx(tx);

        let mut by_name = BTreeMap::new();
        for schema in schemas {
            by_name.insert(schema.table_name.to_string(), schema.table_id);
        }

        self.base_table_ids.clear();
        for table in &self.base_schema.tables {
            let table_id = by_name
                .get(&table.name)
                .copied()
                .ok_or_else(|| format!("base table '{}' missing after reopen", table.name))?;
            self.base_table_ids.push(table_id);
        }

        self.dynamic_tables.retain(|_slot, state| {
            if let Some(table_id) = by_name.get(&state.name).copied() {
                state.table_id = table_id;
                true
            } else {
                false
            }
        });

        Ok(())
    }

    fn execute_table_op(&mut self, interaction: &TableWorkloadInteraction) -> Result<TableObservation, String> {
        match std::panic::catch_unwind(AssertUnwindSafe(|| self.execute_table_op_inner(interaction))) {
            Ok(Ok(observation)) => {
                self.stats.record_table_operation(&interaction.op);
                Ok(observation)
            }
            Ok(Err(err)) => Err(err),
            Err(payload) => Err(format!(
                "[DatastoreNeverPanics] interaction panicked: interaction={interaction:?}, payload={}",
                panic_payload_to_string(&payload)
            )),
        }
    }

    fn execute_table_op_inner(&mut self, interaction: &TableWorkloadInteraction) -> Result<TableObservation, String> {
        trace!(step = self.step, ?interaction, "table interaction");
        match &interaction.op {
            TableOperation::BeginTx { conn } => {
                self.execution.ensure_known_connection(*conn)?;
                if self.read_tx_by_connection[*conn].is_some() {
                    return Err(format!("connection {conn} already has open read transaction"));
                }
                if self.execution.tx_by_connection[*conn].is_some() {
                    return Err(format!("connection {conn} already has open transaction"));
                }
                if let Some(owner) = self.execution.active_writer {
                    return Err(format!(
                        "connection {conn} cannot begin write transaction while connection {owner} owns lock"
                    ));
                }
                self.execution.tx_by_connection[*conn] = Some(
                    self.db()?
                        .begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests),
                );
                self.execution.active_writer = Some(*conn);
                self.stats.transactions.explicit_begin += 1;
                Ok(TableObservation::Applied)
            }
            TableOperation::BeginReadTx { conn } => {
                self.execution.ensure_known_connection(*conn)?;
                if self.execution.tx_by_connection[*conn].is_some() {
                    return Err(format!("connection {conn} already has open write transaction"));
                }
                if self.read_tx_by_connection[*conn].is_some() {
                    return Err(format!("connection {conn} already has open read transaction"));
                }
                let tx = self.db()?.begin_tx(Workload::ForTests);
                self.read_tx_by_connection[*conn] = Some(tx);
                self.stats.record_read_tx();
                Ok(TableObservation::Applied)
            }
            TableOperation::ReleaseReadTx { conn } => {
                self.execution.ensure_known_connection(*conn)?;
                let tx = self.read_tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no read transaction to release"))?;
                let _ = self.db()?.release_tx(tx);
                Ok(TableObservation::Applied)
            }
            TableOperation::BeginTxConflict { owner, conn } => {
                self.expect_write_conflict(*owner, *conn)?;
                Ok(TableObservation::ExpectedError(ExpectedErrorKind::WriteConflict))
            }
            TableOperation::WriteConflictInsert {
                owner,
                conn,
                table,
                row,
            } => {
                self.expect_write_conflict(*owner, *conn)?;
                let err = self
                    .with_mut_tx(*conn, |engine, tx| {
                        let table_id = engine.table_id_for_index(*table)?;
                        let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                        engine
                            .db()?
                            .insert(tx, table_id, &bsatn)
                            .map_err(|err| format!("conflicting insert unexpectedly reached datastore: {err}"))?;
                        Ok(())
                    })
                    .expect_err("active writer should reject conflicting auto-commit write");
                if !err.contains("owns lock") {
                    return Err(format!("write conflict returned wrong error: {err}"));
                }
                Ok(TableObservation::ExpectedError(ExpectedErrorKind::WriteConflict))
            }
            TableOperation::CommitTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "commit")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
                let committed = self
                    .db()?
                    .commit_tx(tx)
                    .map_err(|err| format!("commit interaction failed: {err}"))?;
                self.record_committed_offset(committed.as_ref().map(|(tx_offset, ..)| *tx_offset));
                self.execution.active_writer = None;
                self.stats.transactions.explicit_commit += 1;
                Ok(TableObservation::CommitOrRollback)
            }
            TableOperation::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.db()?.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                self.stats.transactions.explicit_rollback += 1;
                Ok(TableObservation::CommitOrRollback)
            }
            TableOperation::Insert { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                let inserted_row = self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    let (_, row_ref, _) = engine
                        .db()?
                        .insert(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(SimRow::from_product_value(row_ref.to_product_value()))
                })?;
                if !in_tx {
                    self.refresh_observed_durable_offset(false)?;
                }
                Ok(TableObservation::RowInserted {
                    conn: *conn,
                    table: *table,
                    row: inserted_row,
                    in_tx,
                })
            }
            TableOperation::Delete { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    let deleted = engine.db()?.delete_by_rel(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                })?;
                if !in_tx {
                    self.refresh_observed_durable_offset(false)?;
                }
                Ok(TableObservation::RowDeleted {
                    conn: *conn,
                    table: *table,
                    row: row.clone(),
                    in_tx,
                })
            }
            TableOperation::ExactDuplicateInsert { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                let before = self.collect_rows_in_connection(*conn, *table)?;
                let inserted_row = self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = engine.table_id_for_index(*table)?;
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    let (_, row_ref, _) = engine
                        .db()?
                        .insert(tx, table_id, &bsatn)
                        .map_err(|err| format!("exact duplicate insert failed: {err}"))?;
                    Ok(SimRow::from_product_value(row_ref.to_product_value()))
                })?;
                if !in_tx {
                    self.refresh_observed_durable_offset(false)?;
                }
                let after = self.collect_rows_in_connection(*conn, *table)?;
                if &inserted_row != row {
                    return Err(format!(
                        "[ExactDuplicateInsertNoOp] returned row mismatch: expected={row:?}, actual={inserted_row:?}; interaction={interaction:?}"
                    ));
                }
                if after != before {
                    return Err(format!(
                        "[ExactDuplicateInsertNoOp] changed visible rows: before={before:?}, after={after:?}; interaction={interaction:?}"
                    ));
                }
                Ok(TableObservation::Applied)
            }
            TableOperation::UniqueKeyConflictInsert { conn, table, row } => {
                let outcome = self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    match engine.db()?.insert(tx, table_id, &bsatn) {
                        Ok(_) => Ok(Err("unique-key conflict insert unexpectedly succeeded".to_string())),
                        Err(err) if is_unique_constraint_violation(&err) => Ok(Ok(())),
                        Err(err) => Ok(Err(format!(
                            "unique-key conflict insert returned wrong error: expected={:?}, actual={err}",
                            ExpectedErrorKind::UniqueConstraintViolation
                        ))),
                    }
                })?;
                match outcome {
                    Ok(()) => Ok(TableObservation::ExpectedError(
                        ExpectedErrorKind::UniqueConstraintViolation,
                    )),
                    Err(err) => Err(format!("[ExpectedErrorMatches] {err}; interaction={interaction:?}")),
                }
            }
            TableOperation::DeleteMissing { conn, table, row } => {
                let deleted = self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    Ok(engine.db()?.delete_by_rel(tx, table_id, [row.to_product_value()]))
                })?;
                if deleted == 0 {
                    Ok(TableObservation::ExpectedError(ExpectedErrorKind::MissingRow))
                } else {
                    Err(format!(
                        "[ExpectedErrorDoesNotMutate] missing delete removed {deleted} rows; interaction={interaction:?}"
                    ))
                }
            }
            TableOperation::BatchInsert { conn, table, rows } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    for row in rows {
                        let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                        engine
                            .db()?
                            .insert(tx, table_id, &bsatn)
                            .map_err(|err| format!("batch insert failed: {err}"))?;
                    }
                    Ok(())
                })?;
                if !in_tx {
                    self.refresh_observed_durable_offset(false)?;
                }
                Ok(TableObservation::Applied)
            }
            TableOperation::BatchDelete { conn, table, rows } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    for row in rows {
                        let deleted = engine.db()?.delete_by_rel(tx, table_id, [row.to_product_value()]);
                        if deleted != 1 {
                            return Err(format!("batch delete expected 1 row, got {deleted} for row={row:?}"));
                        }
                    }
                    Ok(())
                })?;
                if !in_tx {
                    self.refresh_observed_durable_offset(false)?;
                }
                Ok(TableObservation::Applied)
            }
            TableOperation::Reinsert { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    let deleted = engine.db()?.delete_by_rel(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("reinsert delete expected 1 row, got {deleted} for row={row:?}"));
                    }
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    engine
                        .db()?
                        .insert(tx, table_id, &bsatn)
                        .map_err(|err| format!("reinsert insert failed: {err}"))?;
                    Ok(())
                })?;
                if !in_tx {
                    self.refresh_observed_durable_offset(false)?;
                }
                Ok(TableObservation::Applied)
            }
            TableOperation::AddColumn {
                conn,
                table,
                column,
                default,
            } => {
                let table_id = self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = engine.table_id_for_index(*table)?;
                    let column_idx = engine.base_schema.tables[*table].columns.len() as u16;
                    let mut columns = engine.base_schema.tables[*table]
                        .columns
                        .iter()
                        .enumerate()
                        .map(|(idx, existing)| ColumnSchema::for_test(idx as u16, &existing.name, existing.ty.clone()))
                        .collect::<Vec<_>>();
                    columns.push(ColumnSchema::for_test(column_idx, &column.name, column.ty.clone()));
                    let new_table_id = engine
                        .db()?
                        .add_columns_to_table(tx, table_id, columns, vec![default.clone()])
                        .map_err(|err| format!("add column failed: {err}"))?;
                    Ok(new_table_id)
                })?;
                self.base_table_ids[*table] = table_id;
                self.base_schema.tables[*table].columns.push(column.clone());
                self.refresh_observed_durable_offset(false)?;
                Ok(TableObservation::Applied)
            }
            TableOperation::AddIndex { conn, table, cols } => {
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = engine.table_id_for_index(*table)?;
                    let mut schema = IndexSchema::for_test(
                        format!(
                            "{}_dst_added_{}_idx",
                            engine.base_schema.tables[*table].name,
                            engine.base_schema.tables[*table].extra_indexes.len()
                        ),
                        BTreeAlgorithm::from(cols.iter().copied().collect::<spacetimedb_primitives::ColList>()),
                    );
                    schema.table_id = table_id;
                    engine
                        .db()?
                        .create_index(tx, schema, false)
                        .map_err(|err| format!("add index failed: {err}"))?;
                    Ok(())
                })?;
                if !self.base_schema.tables[*table].extra_indexes.contains(cols) {
                    self.base_schema.tables[*table].extra_indexes.push(cols.clone());
                }
                self.refresh_observed_durable_offset(false)?;
                Ok(TableObservation::Applied)
            }
            TableOperation::PointLookup { conn, table, id } => {
                let actual = self.lookup_base_row(*conn, *table, *id)?;
                Ok(TableObservation::PointLookup {
                    conn: *conn,
                    table: *table,
                    id: *id,
                    actual,
                })
            }
            TableOperation::PredicateCount {
                conn,
                table,
                col,
                value,
            } => {
                let actual = self.count_by_col_eq_in_connection(*conn, *table, *col, value)?;
                Ok(TableObservation::PredicateCount {
                    conn: *conn,
                    table: *table,
                    col: *col,
                    value: value.clone(),
                    actual,
                })
            }
            TableOperation::RangeScan {
                conn,
                table,
                cols,
                lower,
                upper,
            } => {
                let actual = self.range_scan_in_connection(*conn, *table, cols, lower.clone(), upper.clone())?;
                Ok(TableObservation::RangeScan {
                    conn: *conn,
                    table: *table,
                    cols: cols.clone(),
                    lower: lower.clone(),
                    upper: upper.clone(),
                    actual,
                })
            }
            TableOperation::FullScan { conn, table } => {
                let actual = self.collect_rows_in_connection(*conn, *table)?;
                Ok(TableObservation::FullScan {
                    conn: *conn,
                    table: *table,
                    actual,
                })
            }
        }
    }

    fn with_mut_tx<T>(
        &mut self,
        conn: usize,
        mut f: impl FnMut(&mut Self, &mut RelMutTx) -> Result<T, String>,
    ) -> Result<T, String> {
        self.execution.ensure_known_connection(conn)?;
        if self.read_tx_by_connection[conn].is_some() {
            return Err(format!("connection {conn} cannot write while read transaction is open"));
        }
        if self.execution.tx_by_connection[conn].is_some() {
            let mut tx = self.execution.tx_by_connection[conn]
                .take()
                .ok_or_else(|| format!("connection {conn} missing transaction handle"))?;
            let value = f(self, &mut tx)?;
            self.execution.tx_by_connection[conn] = Some(tx);
            return Ok(value);
        }

        if let Some(owner) = self.execution.active_writer {
            return Err(format!(
                "connection {conn} cannot auto-commit write while connection {owner} owns lock"
            ));
        }

        let mut tx = self
            .db()?
            .begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        self.execution.active_writer = Some(conn);
        let value = f(self, &mut tx)?;
        let committed = self
            .db()?
            .commit_tx(tx)
            .map_err(|err| format!("auto-commit write failed: {err}"))?;
        self.record_committed_offset(committed.as_ref().map(|(tx_offset, ..)| *tx_offset));
        self.execution.active_writer = None;
        self.stats.transactions.auto_commit += 1;
        Ok(value)
    }

    fn expect_write_conflict(&self, owner: usize, conn: usize) -> Result<(), String> {
        self.execution.ensure_known_connection(owner)?;
        self.execution.ensure_known_connection(conn)?;
        if owner == conn {
            return Err(format!("write conflict owner and contender are both connection {conn}"));
        }
        if self.execution.active_writer != Some(owner) {
            return Err(format!(
                "expected connection {owner} to own write lock, actual={:?}",
                self.execution.active_writer
            ));
        }
        if self.read_tx_by_connection[conn].is_some() {
            return Err(format!(
                "conflicting connection {conn} unexpectedly has a read transaction"
            ));
        }
        Ok(())
    }

    fn create_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<CommitlogObservation, String> {
        if self.execution.active_writer.is_some() {
            trace!(
                step = self.step,
                slot,
                "skip create dynamic table while transaction is open"
            );
            return Ok(CommitlogObservation::Skipped);
        }
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "create dynamic table");
        self.with_mut_tx(conn, |engine, tx| {
            if engine.dynamic_tables.contains_key(&slot) {
                return Ok(());
            }
            let name = dynamic_table_name(slot);
            let schema = dynamic_schema(&name, 0);
            let table_id = engine
                .db()?
                .create_table(tx, schema)
                .map_err(|err| format!("create dynamic table slot={slot} failed: {err}"))?;
            let seed_row = SimRow {
                values: vec![AlgebraicValue::I64(0), AlgebraicValue::U64(slot as u64)],
            };
            let bsatn = seed_row.to_bsatn().map_err(|err| err.to_string())?;
            engine
                .db()?
                .insert(tx, table_id, &bsatn)
                .map_err(|err| format!("seed dynamic table auto-inc insert failed for slot={slot}: {err}"))?;
            engine.dynamic_tables.insert(
                slot,
                DynamicTableState {
                    name,
                    version: 0,
                    table_id,
                },
            );
            Ok(())
        })?;
        self.refresh_observed_durable_offset(false)?;
        Ok(CommitlogObservation::Applied)
    }

    fn drop_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<CommitlogObservation, String> {
        if self.execution.active_writer.is_some() {
            trace!(
                step = self.step,
                slot,
                "skip drop dynamic table while transaction is open"
            );
            return Ok(CommitlogObservation::Skipped);
        }
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "drop dynamic table");
        self.with_mut_tx(conn, |engine, tx| {
            let Some(state) = engine.dynamic_tables.remove(&slot) else {
                return Ok(());
            };
            if let Err(err) = engine.db()?.drop_table(tx, state.table_id) {
                let msg = err.to_string();
                if !msg.contains("not found") {
                    return Err(format!("drop dynamic table slot={slot} failed: {err}"));
                }
            }
            Ok(())
        })?;
        self.refresh_observed_durable_offset(false)?;
        Ok(CommitlogObservation::Applied)
    }

    fn migrate_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<CommitlogObservation, String> {
        if self.execution.active_writer.is_some() {
            trace!(
                step = self.step,
                slot,
                "skip migrate dynamic table while transaction is open"
            );
            return Ok(CommitlogObservation::Skipped);
        }
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "migrate dynamic table");
        let probe = self.with_mut_tx(conn, |engine, tx| {
            let Some(state) = engine.dynamic_tables.get(&slot).cloned() else {
                return Ok(None);
            };
            let to_version = state.version.saturating_add(1);
            let new_table_id = engine
                .db()?
                .add_columns_to_table(
                    tx,
                    state.table_id,
                    dynamic_column_schemas(to_version),
                    vec![AlgebraicValue::Bool(false)],
                )
                .map_err(|err| format!("migrate add_columns_to_table failed for slot={slot}: {err}"))?;
            let existing_rows = engine
                .db()?
                .iter_mut(tx, new_table_id)
                .map_err(|err| format!("migrate scan table failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();

            let probe_row = dynamic_probe_row(slot, to_version);
            let bsatn = probe_row.to_bsatn().map_err(|err| err.to_string())?;
            let (_, inserted_ref, _) = engine
                .db()?
                .insert(tx, new_table_id, &bsatn)
                .map_err(|err| format!("migrate auto-inc probe failed for slot={slot}: {err}"))?;
            let inserted = SimRow::from_product_value(inserted_ref.to_product_value());
            engine.dynamic_tables.insert(
                slot,
                DynamicTableState {
                    name: state.name,
                    version: to_version,
                    table_id: new_table_id,
                },
            );
            Ok(Some(DynamicMigrationProbe {
                slot,
                from_version: state.version,
                to_version,
                existing_rows,
                inserted_row: inserted,
            }))
        })?;
        self.refresh_observed_durable_offset(false)?;
        Ok(probe
            .map(CommitlogObservation::DynamicMigrationProbe)
            .unwrap_or(CommitlogObservation::Skipped))
    }

    fn normalize_conn(&self, conn: usize) -> usize {
        self.execution.active_writer.unwrap_or(conn)
    }

    fn refresh_observed_durable_offset(&mut self, forced: bool) -> Result<(), String> {
        let durable_offset = self.durability.durable_tx_offset().last_seen();
        if forced || durable_offset != self.last_observed_durable_offset {
            self.last_observed_durable_offset = durable_offset;
        }
        Ok(())
    }

    async fn wait_for_requested_durability(&mut self, forced: bool) -> Result<(), String> {
        if let Some(target_offset) = self.last_requested_durable_offset {
            let current = self.durability.durable_tx_offset().last_seen();
            if current.is_none_or(|offset| offset < target_offset) {
                self.durability
                    .durable_tx_offset()
                    .wait_for(target_offset)
                    .await
                    .map_err(|err| format!("durability wait for tx offset {target_offset} failed: {err}"))?;
            }
        } else if forced {
            tokio::task::yield_now().await;
        }
        self.refresh_observed_durable_offset(forced)
    }

    fn record_committed_offset(&mut self, offset: Option<u64>) {
        if let Some(offset) = offset {
            self.last_requested_durable_offset = Some(offset);
        }
    }

    fn table_id_for_index(&self, table: usize) -> Result<TableId, String> {
        self.base_table_ids
            .get(table)
            .copied()
            .ok_or_else(|| format!("table {table} out of range"))
    }

    fn lookup_base_row(&self, conn: usize, table: usize, id: u64) -> Result<Option<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn) {
            Ok(self
                .db()?
                .iter_by_col_eq_mut(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("in-tx lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next())
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn) {
            Ok(self
                .db()?
                .iter_by_col_eq(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("read-tx lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next())
        } else {
            let db = self.db()?;
            let tx = db.begin_tx(Workload::ForTests);
            self.stats.record_read_tx();
            let found = self
                .db()?
                .iter_by_col_eq(&tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next();
            let _ = db.release_tx(tx);
            Ok(found)
        }
    }

    fn collect_rows_in_connection(&self, conn: usize, table: usize) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn) {
            let mut rows = self
                .db()?
                .iter_mut(tx, table_id)
                .map_err(|err| format!("in-tx scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn) {
            let mut rows = self
                .db()?
                .iter(tx, table_id)
                .map_err(|err| format!("read-tx scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        } else {
            self.collect_rows_by_id(table_id)
        }
    }

    fn count_by_col_eq_in_connection(
        &self,
        conn: usize,
        table: usize,
        col: u16,
        value: &AlgebraicValue,
    ) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn) {
            Ok(self
                .db()?
                .iter_by_col_eq_mut(tx, table_id, col, value)
                .map_err(|err| format!("in-tx predicate query failed: {err}"))?
                .count())
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn) {
            Ok(self
                .db()?
                .iter_by_col_eq(tx, table_id, col, value)
                .map_err(|err| format!("read-tx predicate query failed: {err}"))?
                .count())
        } else {
            self.count_by_col_eq_for_property(table, col, value)
        }
    }

    fn range_scan_in_connection(
        &self,
        conn: usize,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        let col_list = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let mut rows = if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn) {
            self.db()?
                .iter_by_col_range_mut(tx, table_id, col_list, (lower, upper))
                .map_err(|err| format!("in-tx range scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>()
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn) {
            self.db()?
                .iter_by_col_range(tx, table_id, col_list, (lower, upper))
                .map_err(|err| format!("read-tx range scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>()
        } else {
            let db = self.db()?;
            let tx = db.begin_tx(Workload::ForTests);
            self.stats.record_read_tx();
            let rows = self
                .db()?
                .iter_by_col_range(&tx, table_id, col_list, (lower, upper))
                .map_err(|err| format!("range scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            let _ = db.release_tx(tx);
            rows
        };
        rows.sort_by(|lhs, rhs| compare_rows_for_range(lhs, rhs, cols));
        Ok(rows)
    }

    fn count_rows_for_property(&self, table: usize) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
        self.stats.record_read_tx();
        let total = self
            .db()?
            .iter(&tx, table_id)
            .map_err(|err| format!("scan failed: {err}"))?
            .count();
        let _ = db.release_tx(tx);
        Ok(total)
    }

    fn count_by_col_eq_for_property(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
        self.stats.record_read_tx();
        let total = self
            .db()?
            .iter_by_col_eq(&tx, table_id, col, value)
            .map_err(|err| format!("predicate query failed: {err}"))?
            .count();
        let _ = db.release_tx(tx);
        Ok(total)
    }

    fn range_scan_for_property(
        &self,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
        self.stats.record_read_tx();
        let cols = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let rows = self
            .db()?
            .iter_by_col_range(&tx, table_id, cols, (lower, upper))
            .map_err(|err| format!("range scan failed: {err}"))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
        let _ = db.release_tx(tx);
        Ok(rows)
    }

    fn collect_rows_by_id(&self, table_id: TableId) -> Result<Vec<SimRow>, String> {
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
        self.stats.record_read_tx();
        let mut rows = self
            .db()?
            .iter(&tx, table_id)
            .map_err(|err| format!("scan failed: {err}"))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
        let _ = db.release_tx(tx);
        rows.sort_by_key(|row| row.id().unwrap_or_default());
        Ok(rows)
    }

    fn durable_replay_summary(&self) -> Result<DurableReplaySummary, String> {
        Ok(DurableReplaySummary {
            durable_offset: self.last_observed_durable_offset,
            base_rows: self.collect_base_rows()?,
            dynamic_table_count: self.dynamic_tables.len(),
        })
    }

    async fn reopen_for_final_replay_check(&mut self) -> Result<DurableReplaySummary, String> {
        let old_db = self
            .db
            .take()
            .ok_or_else(|| "final replay check failed: relational db not initialized".to_string())?;
        old_db.shutdown().await;
        drop(old_db);

        let (durability, db) = self.reopen_from_history_with_fault_retry("final replay check")?;
        self.stats.runtime.durability_actors_started += 1;
        self.durability = durability;
        self.db = Some(db);
        self.rebuild_table_handles_after_reopen()?;
        self.last_observed_durable_offset = self.durability.durable_tx_offset().last_seen();
        self.durable_replay_summary()
    }

    async fn collect_outcome(&mut self) -> Result<RelationalDbCommitlogOutcome, String> {
        self.wait_for_requested_durability(true).await?;
        let table = self.collect_table_outcome()?;
        let replay = self.reopen_for_final_replay_check().await?;
        let durable_commit_count = self
            .last_observed_durable_offset
            .map(|offset| (offset as usize).saturating_add(1))
            .unwrap_or(0);
        let replay_table_count = replay.base_rows.len() + replay.dynamic_table_count;
        debug!(durable_commits = durable_commit_count, "replayed durable prefix");
        Ok(RelationalDbCommitlogOutcome {
            applied_steps: self.step,
            durable_commit_count,
            replay_table_count,
            schema: schema_summary(&self.base_schema),
            interactions: self.stats.interactions.clone(),
            table_ops: self.stats.table_ops.clone(),
            transactions: self.stats.transaction_summary(durable_commit_count),
            runtime: self.stats.runtime_summary(),
            disk_faults: self.commitlog_repo.fault_summary(),
            replay,
            table,
        })
    }

    fn collect_base_rows(&self) -> Result<Vec<Vec<SimRow>>, String> {
        self.base_table_ids
            .iter()
            .map(|&table_id| self.collect_rows_by_id(table_id))
            .collect()
    }

    fn collect_table_outcome(&self) -> Result<TableWorkloadOutcome, String> {
        let mut final_rows = Vec::with_capacity(self.base_table_ids.len());
        let mut final_row_counts = Vec::with_capacity(self.base_table_ids.len());

        for &table_id in &self.base_table_ids {
            let rows = self.collect_rows_by_id(table_id)?;
            final_row_counts.push(rows.len() as u64);
            final_rows.push(rows);
        }

        Ok(TableWorkloadOutcome {
            final_row_counts,
            final_rows,
        })
    }

    fn finish(&mut self) {
        for tx in &mut self.execution.tx_by_connection {
            if let Some(tx) = tx.take()
                && let Some(db) = &self.db
            {
                let _ = db.rollback_mut_tx(tx);
            }
        }
        for tx in &mut self.read_tx_by_connection {
            if let Some(tx) = tx.take()
                && let Some(db) = &self.db
            {
                let _ = db.release_tx(tx);
            }
        }
        self.execution.active_writer = None;
    }

    fn db(&self) -> Result<&RelationalDB, String> {
        self.db
            .as_ref()
            .ok_or_else(|| "relational db is unavailable during close/reopen".to_string())
    }
}

impl TargetPropertyAccess for RelationalDbEngine {
    fn schema_plan(&self) -> &SchemaPlan {
        &self.base_schema
    }

    fn lookup_in_connection(&self, conn: usize, table: usize, id: u64) -> Result<Option<SimRow>, String> {
        Self::lookup_base_row(self, conn, table, id)
    }

    fn collect_rows_for_table(&self, table: usize) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        Self::collect_rows_by_id(self, table_id)
    }

    fn count_rows(&self, table: usize) -> Result<usize, String> {
        Self::count_rows_for_property(self, table)
    }

    fn count_by_col_eq(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String> {
        Self::count_by_col_eq_for_property(self, table, col, value)
    }

    fn range_scan(
        &self,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String> {
        Self::range_scan_for_property(self, table, cols, lower, upper)
    }
}

impl TargetEngine<CommitlogInteraction> for RelationalDbEngine {
    type Observation = CommitlogObservation;
    type Outcome = RelationalDbCommitlogOutcome;
    type Error = String;

    #[allow(clippy::manual_async_fn)]
    fn execute_interaction<'a>(
        &'a mut self,
        interaction: &'a CommitlogInteraction,
    ) -> impl std::future::Future<Output = Result<Self::Observation, Self::Error>> + 'a {
        async move { self.execute(interaction).await }
    }

    fn finish(&mut self) {
        Self::finish(self);
    }

    #[allow(clippy::manual_async_fn)]
    fn collect_outcome<'a>(&'a mut self) -> impl std::future::Future<Output = anyhow::Result<Self::Outcome>> + 'a {
        async move {
            RelationalDbEngine::collect_outcome(self)
                .await
                .map_err(anyhow::Error::msg)
        }
    }
}

type StressCommitlogRepo = BuggifiedRepo<MemoryCommitlogRepo>;
type InMemoryCommitlogDurability = Local<ProductValue, StressCommitlogRepo>;

struct RelationalDbBootstrap {
    db: RelationalDB,
    runtime_handle: tokio::runtime::Handle,
    commitlog_repo: StressCommitlogRepo,
    durability: Arc<InMemoryCommitlogDurability>,
    durability_opts: spacetimedb_durability::local::Options,
    runtime_guard: Option<tokio::runtime::Runtime>,
}

fn bootstrap_relational_db(
    seed: DstSeed,
    fault_profile: CommitlogFaultProfile,
) -> anyhow::Result<RelationalDbBootstrap> {
    let (runtime_handle, runtime_guard) = if let Ok(handle) = tokio::runtime::Handle::try_current() {
        (handle, None)
    } else {
        let runtime = tokio::runtime::Runtime::new()?;
        (runtime.handle().clone(), Some(runtime))
    };
    let fault_config = CommitlogFaultConfig::for_profile(fault_profile);
    configure_madsim_buggify(fault_config.enabled());

    let commitlog_repo = BuggifiedRepo::new(MemoryCommitlogRepo::new(8 * 1024 * 1024), fault_config);
    let durability_opts = commitlog_stress_options(seed.fork(701));
    let durability = Arc::new(
        InMemoryCommitlogDurability::open_with_repo(commitlog_repo.clone(), runtime_handle.clone(), durability_opts)
            .map_err(|err| anyhow::anyhow!("open in-memory durability failed: {err}"))?,
    );
    let persistence = Persistence {
        durability: durability.clone(),
        disk_size: Arc::new(in_memory_size_on_disk),
        snapshots: None,
        runtime: runtime_handle.clone(),
    };
    let (db, connected_clients) = RelationalDB::open(
        Identity::ZERO,
        Identity::ZERO,
        EmptyHistory::new(),
        Some(persistence),
        None,
        PagePool::new_for_test(),
    )?;
    assert_eq!(connected_clients.len(), 0);
    db.with_auto_commit(Workload::Internal, |tx| {
        db.set_initialized(tx, Program::empty(HostType::Wasm.into()))
    })?;
    Ok(RelationalDbBootstrap {
        db,
        runtime_handle,
        commitlog_repo,
        durability,
        durability_opts,
        runtime_guard,
    })
}

fn commitlog_stress_options(seed: DstSeed) -> spacetimedb_durability::local::Options {
    let mut opts = spacetimedb_durability::local::Options::default();
    opts.commitlog.max_segment_size = 2 * 1024;
    opts.commitlog.offset_index_interval_bytes = NonZeroU64::new(256).expect("256 > 0");
    opts.commitlog.offset_index_require_segment_fsync = seed.0.is_multiple_of(2);
    opts.commitlog.write_buffer_size = 512;
    opts
}

fn configure_madsim_buggify(enabled: bool) {
    #[cfg(madsim)]
    {
        if enabled {
            madsim::buggify::enable();
        } else {
            madsim::buggify::disable();
        }
    }
    #[cfg(not(madsim))]
    let _ = enabled;
}

fn runtime_alive_tasks() -> Option<usize> {
    // The madsim runtime exposes live task metrics on `Runtime`, but the target
    // only receives Tokio-compatible handles. Keep this explicit instead of
    // reporting madsim-tokio's dummy zero-valued metrics as real data.
    None
}

fn schema_summary(schema: &SchemaPlan) -> SchemaSummary {
    let initial_tables = schema.tables.len();
    let initial_columns = schema.tables.iter().map(|table| table.columns.len()).sum();
    let max_columns_per_table = schema
        .tables
        .iter()
        .map(|table| table.columns.len())
        .max()
        .unwrap_or_default();
    let extra_indexes = schema
        .tables
        .iter()
        .map(|table| table.extra_indexes.len())
        .sum::<usize>();
    SchemaSummary {
        initial_tables,
        initial_columns,
        max_columns_per_table,
        initial_indexes: initial_tables + extra_indexes,
        extra_indexes,
    }
}

fn in_memory_size_on_disk() -> io::Result<SizeOnDisk> {
    Ok(SizeOnDisk::default())
}

fn is_unique_constraint_violation(err: &DBError) -> bool {
    matches!(
        err,
        DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_)))
    )
}

fn panic_payload_to_string(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else {
        "<non-string panic payload>".to_string()
    }
}

fn compare_rows_for_range(lhs: &SimRow, rhs: &SimRow, cols: &[u16]) -> std::cmp::Ordering {
    lhs.project_key(cols)
        .to_algebraic_value()
        .cmp(&rhs.project_key(cols).to_algebraic_value())
        .then_with(|| lhs.values.cmp(&rhs.values))
}

fn dynamic_table_name(slot: u32) -> String {
    format!("dst_dynamic_slot_{slot}")
}

fn dynamic_column_schemas(version: u32) -> Vec<ColumnSchema> {
    let mut columns = vec![
        ColumnSchema::for_test(0, "id", AlgebraicType::I64),
        ColumnSchema::for_test(1, "value", AlgebraicType::U64),
    ];
    for v in 1..=version {
        columns.push(ColumnSchema::for_test(
            (v + 1) as u16,
            format!("migrated_v{v}"),
            AlgebraicType::Bool,
        ));
    }
    columns
}

fn dynamic_probe_row(slot: u32, version: u32) -> SimRow {
    let mut values = vec![AlgebraicValue::I64(0), AlgebraicValue::U64(slot as u64)];
    for _ in 1..=version {
        values.push(AlgebraicValue::Bool(false));
    }
    SimRow { values }
}

fn dynamic_schema(name: &str, version: u32) -> TableSchema {
    let columns = dynamic_column_schemas(version);
    let indexes = vec![IndexSchema::for_test(format!("{name}_id_idx"), BTreeAlgorithm::from(0))];
    let constraints = vec![ConstraintSchema::unique_for_test(format!("{name}_id_unique"), 0)];
    let sequences = vec![SequenceSchema {
        sequence_id: SequenceId::SENTINEL,
        sequence_name: format!("{name}_id_seq").into(),
        table_id: TableId::SENTINEL,
        col_pos: 0.into(),
        increment: 1,
        start: 1,
        min_value: 1,
        max_value: i128::MAX,
    }];
    TableSchema::new(
        TableId::SENTINEL,
        TableName::for_test(name),
        None,
        columns,
        indexes,
        constraints,
        sequences,
        StTableType::User,
        StAccess::Public,
        None,
        Some(0.into()),
        false,
        None,
    )
}
