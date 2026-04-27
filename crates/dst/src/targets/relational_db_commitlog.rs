//! RelationalDB DST target with mocked commitlog file chaos and replay checks.

use std::{
    collections::BTreeMap,
    ops::Bound,
    sync::Arc,
    thread::sleep,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use spacetimedb_core::{
    db::relational_db::{MutTx as RelMutTx, Persistence, RelationalDB, Txdata},
    messages::control_db::HostType,
};
use spacetimedb_datastore::{
    execution_context::Workload,
    traits::{IsolationLevel, Program},
};
use spacetimedb_durability::{EmptyHistory, History};
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_paths::{server::ReplicaDir, FromPathUnchecked};
use spacetimedb_primitives::{SequenceId, TableId};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;
use tracing::{debug, info, trace};

use crate::{
    config::RunConfig,
    core::NextInteractionSource,
    schema::{SchemaPlan, SimRow},
    seed::DstSeed,
    targets::properties::{PropertyRuntime, TargetPropertyAccess},
    workload::{
        commitlog_ops::{CommitlogInteraction, CommitlogWorkloadOutcome},
        table_ops::{ConnectionWriteState, TableScenario, TableScenarioId, TableWorkloadInteraction},
    },
};

pub type RelationalDbCommitlogOutcome = CommitlogWorkloadOutcome;

pub fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<RelationalDbCommitlogOutcome> {
    let mut connection_rng = seed.fork(121).rng();
    let num_connections = connection_rng.index(3) + 1;
    let mut schema_rng = seed.fork(122).rng();
    let schema = scenario.generate_schema(&mut schema_rng);
    let mut generator = crate::workload::commitlog_ops::NextInteractionGeneratorComposite::new(
        seed,
        scenario,
        schema.clone(),
        num_connections,
        config.max_interactions_or_default(usize::MAX),
    );
    let mut engine = RelationalDbEngine::new(seed, &schema, num_connections)?;
    let deadline = config.deadline();
    let mut step_index = 0usize;

    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            generator.request_finish();
        }
        let Some(interaction) = generator.next_interaction() else {
            break;
        };
        trace!(step_index, ?interaction, "streaming interaction");
        engine
            .execute(&interaction)
            .map_err(|reason| anyhow::anyhow!("workload failed at step {step_index}: {reason}"))?;
        step_index = step_index.saturating_add(1);
    }

    let outcome = engine.collect_outcome().map_err(anyhow::Error::msg)?;
    engine.finish();
    info!(
        applied_steps = outcome.applied_steps,
        durable_commit_count = outcome.durable_commit_count,
        replay_table_count = outcome.replay_table_count,
        "relational_db_commitlog complete"
    );
    Ok(outcome)
}

#[derive(Clone, Debug)]
struct DynamicTableState {
    name: String,
    version: u32,
    table_id: TableId,
}

/// Engine executing mixed table+lifecycle interactions while recording mocked durable history.
struct RelationalDbEngine {
    db: Option<RelationalDB>,
    execution: ConnectionWriteState<RelMutTx>,
    base_schema: SchemaPlan,
    base_table_ids: Vec<TableId>,
    dynamic_tables: BTreeMap<u32, DynamicTableState>,
    step: usize,
    last_observed_durable_offset: Option<u64>,
    last_durable_snapshot: DurableSnapshot,
    pending_snapshot_capture: bool,
    properties: PropertyRuntime,
    runtime_handle: tokio::runtime::Handle,
    replica_dir: ReplicaDir,
    _runtime_guard: Option<tokio::runtime::Runtime>,
}

type DurableSnapshot = BTreeMap<String, Vec<SimRow>>;

impl RelationalDbEngine {
    fn new(seed: DstSeed, schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self> {
        let (db, runtime_handle, replica_dir, runtime_guard) = bootstrap_relational_db(seed.fork(700))?;
        let mut this = Self {
            db: Some(db),
            execution: ConnectionWriteState::new(num_connections),
            base_schema: schema.clone(),
            base_table_ids: Vec::with_capacity(schema.tables.len()),
            dynamic_tables: BTreeMap::new(),
            step: 0,
            last_observed_durable_offset: None,
            last_durable_snapshot: BTreeMap::new(),
            pending_snapshot_capture: false,
            properties: PropertyRuntime::default(),
            runtime_handle,
            replica_dir,
            _runtime_guard: runtime_guard,
        };
        this.install_base_schema().map_err(anyhow::Error::msg)?;
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
        self.db()?
            .commit_tx(tx)
            .map(|_| ())
            .map_err(|err| format!("install base schema commit failed: {err}"))
    }

    fn execute(&mut self, interaction: &CommitlogInteraction) -> Result<(), String> {
        self.step = self.step.saturating_add(1);
        match interaction {
            CommitlogInteraction::Table(op) => self.execute_table_op(op),
            CommitlogInteraction::CreateDynamicTable { conn, slot } => self.create_dynamic_table(*conn, *slot),
            CommitlogInteraction::DropDynamicTable { conn, slot } => self.drop_dynamic_table(*conn, *slot),
            CommitlogInteraction::MigrateDynamicTable { conn, slot } => self.migrate_dynamic_table(*conn, *slot),
            CommitlogInteraction::ChaosSync => self.sync_and_snapshot(true),
            CommitlogInteraction::CloseReopen => self.close_and_reopen(),
        }
    }

    fn close_and_reopen(&mut self) -> Result<(), String> {
        if self.execution.active_writer.is_some() || self.execution.tx_by_connection.iter().any(|tx| tx.is_some()) {
            trace!("skip close/reopen while transaction is open");
            return Ok(());
        }

        self.sync_and_snapshot(true)?;
        // Explicitly drop the current RelationalDB instance before attempting
        // to open a new durability+DB pair on the same replica directory.
        let old_db = self
            .db
            .take()
            .ok_or_else(|| "close/reopen failed: relational db not initialized".to_string())?;
        self.runtime_handle.block_on(old_db.shutdown());
        drop(old_db);
        info!("starting durability");

        // In madsim we avoid blocking close here; dropping the close future
        // triggers actor abort via durability's close guard.

        let durability = Arc::new(
            spacetimedb_durability::Local::open(
                self.replica_dir.clone(),
                self.runtime_handle.clone(),
                Default::default(),
                None,
            )
            .map_err(|err| format!("reopen local durability failed: {err}"))?,
        );

        let persistence = Persistence {
            durability: durability.clone(),
            disk_size: Arc::new({
                let durability = durability.clone();
                move || durability.size_on_disk()
            }),
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
        self.db = Some(db);
        self.rebuild_table_handles_after_reopen()?;
        self.capture_pending_snapshot_if_idle()?;
        debug!(
            base_tables = self.base_table_ids.len(),
            dynamic_tables = self.dynamic_tables.len(),
            "reopened relational db from durable history"
        );
        Ok(())
    }

    fn rebuild_table_handles_after_reopen(&mut self) -> Result<(), String> {
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
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

    fn execute_table_op(&mut self, interaction: &TableWorkloadInteraction) -> Result<(), String> {
        trace!(step = self.step, ?interaction, "table interaction");
        match interaction {
            TableWorkloadInteraction::BeginTx { conn } => {
                self.execution.ensure_known_connection(*conn)?;
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
                Ok(())
            }
            TableWorkloadInteraction::CommitTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "commit")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
                self.db()?
                    .commit_tx(tx)
                    .map_err(|err| format!("commit interaction failed: {err}"))?;
                self.execution.active_writer = None;
                self.capture_pending_snapshot_if_idle()?;
                self.with_property_runtime(|runtime, access| runtime.on_commit_or_rollback(access))?;
                Ok(())
            }
            TableWorkloadInteraction::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.db()?.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                self.capture_pending_snapshot_if_idle()?;
                self.with_property_runtime(|runtime, access| runtime.on_commit_or_rollback(access))?;
                Ok(())
            }
            TableWorkloadInteraction::Insert { conn, table, row } => {
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
                    self.sync_and_snapshot(false)?;
                }
                let step = self.step as u64;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_insert(access, step, *conn, *table, &inserted_row, in_tx)
                })
            }
            TableWorkloadInteraction::Delete { conn, table, row } => {
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
                    self.sync_and_snapshot(false)?;
                }
                let step = self.step as u64;
                self.with_property_runtime(|runtime, access| runtime.on_delete(access, step, *conn, *table, row, in_tx))
            }
        }
    }

    fn with_mut_tx<T>(
        &mut self,
        conn: usize,
        mut f: impl FnMut(&mut Self, &mut RelMutTx) -> Result<T, String>,
    ) -> Result<T, String> {
        self.execution.ensure_known_connection(conn)?;
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
        self.db()?
            .commit_tx(tx)
            .map_err(|err| format!("auto-commit write failed: {err}"))?;
        self.execution.active_writer = None;
        self.capture_pending_snapshot_if_idle()?;
        Ok(value)
    }

    fn create_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<(), String> {
        if self.execution.active_writer.is_some() {
            trace!(
                step = self.step,
                slot,
                "skip create dynamic table while transaction is open"
            );
            return Ok(());
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
        self.sync_and_snapshot(false)
    }

    fn drop_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<(), String> {
        if self.execution.active_writer.is_some() {
            trace!(
                step = self.step,
                slot,
                "skip drop dynamic table while transaction is open"
            );
            return Ok(());
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
        self.sync_and_snapshot(false)
    }

    fn migrate_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<(), String> {
        if self.execution.active_writer.is_some() {
            trace!(
                step = self.step,
                slot,
                "skip migrate dynamic table while transaction is open"
            );
            return Ok(());
        }
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "migrate dynamic table");
        self.with_mut_tx(conn, |engine, tx| {
            let Some(state) = engine.dynamic_tables.get(&slot).cloned() else {
                return Ok(());
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

            // Sequence regression probe:
            // after add-columns migration, force one auto-inc insert.
            // If sequence state was reset by migration, this can collide with existing ids.
            let max_existing_id = existing_rows
                .iter()
                .filter_map(sim_row_integer_id)
                .max()
                .unwrap_or(0);
            let probe_row = dynamic_probe_row(slot, to_version);
            let bsatn = probe_row.to_bsatn().map_err(|err| err.to_string())?;
            let (_, inserted_ref, _) = engine
                .db()?
                .insert(tx, new_table_id, &bsatn)
                .map_err(|err| format!("migrate auto-inc probe failed for slot={slot}: {err}"))?;
            let inserted = SimRow::from_product_value(inserted_ref.to_product_value());
            let inserted_id = sim_row_integer_id(&inserted)
                .ok_or_else(|| format!("migrate probe row missing id: {inserted:?}"))?;
            if inserted_id <= max_existing_id {
                return Err(format!(
                    "migrate auto-inc probe produced non-advancing id for slot={slot}: inserted_id={inserted_id}, max_existing_id={max_existing_id}"
                ));
            }
            engine.dynamic_tables.insert(
                slot,
                DynamicTableState {
                    name: state.name,
                    version: to_version,
                    table_id: new_table_id,
                },
            );
            Ok(())
        })?;
        self.sync_and_snapshot(false)
    }

    fn normalize_conn(&self, conn: usize) -> usize {
        self.execution.active_writer.unwrap_or(conn)
    }

    fn sync_and_snapshot(&mut self, forced: bool) -> Result<(), String> {
        Ok(())
    }

    fn capture_pending_snapshot_if_idle(&mut self) -> Result<(), String> {
        if self.pending_snapshot_capture && self.execution.active_writer.is_none() {
            self.last_durable_snapshot = self.snapshot_tracked_tables()?;
            self.pending_snapshot_capture = false;
        }
        Ok(())
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
        } else {
            let db = self.db()?;
            let tx = db.begin_tx(Workload::ForTests);
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

    fn count_rows_for_property(&self, table: usize) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
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

    fn with_property_runtime<T>(
        &mut self,
        f: impl FnOnce(&mut PropertyRuntime, &Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut runtime = std::mem::take(&mut self.properties);
        let result = f(&mut runtime, self);
        self.properties = runtime;
        result
    }

    fn collect_rows_by_id(&self, table_id: TableId) -> Result<Vec<SimRow>, String> {
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
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

    fn snapshot_tracked_tables(&self) -> Result<DurableSnapshot, String> {
        let mut snap = BTreeMap::new();
        for (idx, table_id) in self.base_table_ids.iter().enumerate() {
            let name = self
                .base_schema
                .tables
                .get(idx)
                .map(|t| t.name.clone())
                .ok_or_else(|| format!("base table index {idx} missing schema"))?;
            snap.insert(name, self.collect_rows_by_id(*table_id)?);
        }
        for state in self.dynamic_tables.values() {
            let name = state.name.clone();
            snap.insert(name, self.collect_rows_by_id(state.table_id)?);
        }
        Ok(snap)
    }

    fn collect_outcome(&mut self) -> Result<RelationalDbCommitlogOutcome, String> {
        self.capture_pending_snapshot_if_idle()?;
        self.sync_and_snapshot(true)?;
        let durable_commit_count = self
            .last_observed_durable_offset
            .map(|offset| (offset as usize).saturating_add(1))
            .unwrap_or(0);
        debug!(durable_commits = durable_commit_count, "replayed durable prefix");
        Ok(RelationalDbCommitlogOutcome {
            applied_steps: self.step,
            durable_commit_count,
            //TODO: remove 10
            replay_table_count: 10,
        })
    }

    fn finish(&mut self) {
        for tx in &mut self.execution.tx_by_connection {
            if let Some(tx) = tx.take() {
                if let Some(db) = &self.db {
                    let _ = db.rollback_mut_tx(tx);
                }
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

fn reopen_from_history(history: impl History<TxData = Txdata>) -> Result<DurableSnapshot, String> {
    debug!("reopen relational db from mocked durable history");
    let (db, connected_clients) = RelationalDB::open(
        Identity::ZERO,
        Identity::ZERO,
        history,
        None,
        None,
        PagePool::new_for_test(),
    )
    .map_err(|err| format!("reopen from history failed: {err}"))?;
    if !connected_clients.is_empty() {
        return Err(format!(
            "unexpected connected clients after replay: {connected_clients:?}"
        ));
    }

    let tx = db.begin_tx(Workload::ForTests);
    let schemas = db
        .get_all_tables(&tx)
        .map_err(|err| format!("list tables after replay failed: {err}"))?;
    let mut snapshot = BTreeMap::<String, Vec<SimRow>>::new();
    for schema in schemas {
        let name = schema.table_name.to_string();
        if !is_user_dst_table(&name) {
            continue;
        }
        let mut rows = db
            .iter(&tx, schema.table_id)
            .map_err(|err| format!("scan replay table '{name}' failed: {err}"))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.id().unwrap_or_default());
        snapshot.insert(name, rows);
    }
    let _ = db.release_tx(tx);
    debug!(tables = snapshot.len(), "reopen snapshot collected");
    Ok(snapshot)
}

fn is_user_dst_table(name: &str) -> bool {
    !name.starts_with("st_")
}

fn bootstrap_relational_db(
    seed: DstSeed,
) -> anyhow::Result<(
    RelationalDB,
    tokio::runtime::Handle,
    ReplicaDir,
    Option<tokio::runtime::Runtime>,
)> {
    let (runtime_handle, runtime_guard) = if let Ok(handle) = tokio::runtime::Handle::try_current() {
        (handle, None)
    } else {
        let runtime = tokio::runtime::Runtime::new()?;
        (runtime.handle().clone(), Some(runtime))
    };
    let replica_dir = dst_replica_dir(seed)?;
    let durability = Arc::new(
        spacetimedb_durability::Local::open(replica_dir.clone(), runtime_handle.clone(), Default::default(), None)
            .map_err(|err| anyhow::anyhow!("open local durability failed: {err}"))?,
    );
    let persistence = Persistence {
        durability: durability.clone(),
        disk_size: Arc::new(move || durability.size_on_disk()),
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
    Ok((db, runtime_handle, replica_dir, runtime_guard))
}

fn dst_replica_dir(seed: DstSeed) -> anyhow::Result<ReplicaDir> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!(
        "spacetimedb-dst-relational-db-commitlog-{}-{}-{nonce}",
        seed.0,
        std::process::id()
    ));
    std::fs::create_dir_all(&path)?;
    Ok(ReplicaDir::from_path_unchecked(path))
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

fn sim_row_integer_id(row: &SimRow) -> Option<i128> {
    match row.values.first() {
        Some(AlgebraicValue::I64(value)) => Some(*value as i128),
        Some(AlgebraicValue::U64(value)) => Some(*value as i128),
        _ => None,
    }
}
