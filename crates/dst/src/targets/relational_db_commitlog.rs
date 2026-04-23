//! RelationalDB DST target with mocked commitlog file chaos and replay checks.

use std::{
    collections::{BTreeMap, HashMap},
    ops::Bound,
    time::Instant,
};

use spacetimedb_commitlog::{self as commitlog, error::Traversal};
use spacetimedb_core::{
    db::relational_db::{MutTx as RelMutTx, RelationalDB, Txdata},
    messages::control_db::HostType,
};
use spacetimedb_datastore::{
    execution_context::Workload,
    traits::{IsolationLevel, Program, TxData as DatastoreTxData},
};
use spacetimedb_durability::{EmptyHistory, History, TxOffset};
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;
use tracing::{debug, info, trace, warn};

use crate::{
    config::RunConfig,
    core::NextInteractionSource,
    schema::{SchemaPlan, SimRow},
    seed::{DstRng, DstSeed},
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
    let mut engine = RelationalDbCommitlogEngine::new(seed, &schema, num_connections)?;
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
    version: u32,
    table_id: TableId,
}

/// Engine executing mixed table+lifecycle interactions while recording mocked durable history.
struct RelationalDbCommitlogEngine {
    db: RelationalDB,
    execution: ConnectionWriteState<RelMutTx>,
    base_schema: SchemaPlan,
    base_table_ids: Vec<TableId>,
    dynamic_tables: HashMap<u32, DynamicTableState>,
    step: usize,
    commitlog: MockCommitlogFs,
    last_durable_snapshot: DurableSnapshot,
    pending_snapshot_capture: bool,
    properties: PropertyRuntime,
}

type DurableSnapshot = BTreeMap<String, Vec<SimRow>>;

impl RelationalDbCommitlogEngine {
    fn new(seed: DstSeed, schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self> {
        let db = bootstrap_relational_db()?;
        let mut this = Self {
            db,
            execution: ConnectionWriteState::new(num_connections),
            base_schema: schema.clone(),
            base_table_ids: Vec::with_capacity(schema.tables.len()),
            dynamic_tables: HashMap::new(),
            step: 0,
            commitlog: MockCommitlogFs::new(seed.fork(700)),
            last_durable_snapshot: BTreeMap::new(),
            pending_snapshot_capture: false,
            properties: PropertyRuntime::default(),
        };
        this.initialize_program().map_err(anyhow::Error::msg)?;
        this.install_base_schema().map_err(anyhow::Error::msg)?;
        Ok(this)
    }

    fn initialize_program(&mut self) -> Result<(), String> {
        let mut tx = self.db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        self.db
            .set_initialized(&mut tx, Program::empty(HostType::Wasm.into()))
            .map_err(|err| format!("set_initialized failed: {err}"))?;
        self.commit_tx_capture(tx, "initialize")
    }

    fn install_base_schema(&mut self) -> Result<(), String> {
        let mut tx = self.db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
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
                .db
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
        self.commit_tx_capture(tx, "install base schema")
    }

    fn execute(&mut self, interaction: &CommitlogInteraction) -> Result<(), String> {
        self.step = self.step.saturating_add(1);
        match interaction {
            CommitlogInteraction::Table(op) => self.execute_table_op(op),
            CommitlogInteraction::CreateDynamicTable { conn, slot } => self.create_dynamic_table(*conn, *slot),
            CommitlogInteraction::DropDynamicTable { conn, slot } => self.drop_dynamic_table(*conn, *slot),
            CommitlogInteraction::MigrateDynamicTable { conn, slot } => self.migrate_dynamic_table(*conn, *slot),
            CommitlogInteraction::ChaosSync => self.sync_and_snapshot(true),
        }
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
                self.execution.tx_by_connection[*conn] =
                    Some(self.db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests));
                self.execution.active_writer = Some(*conn);
                Ok(())
            }
            TableWorkloadInteraction::CommitTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "commit")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
                self.commit_tx_capture(tx, "commit interaction")?;
                self.execution.active_writer = None;
                self.capture_pending_snapshot_if_idle()?;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_commit_or_rollback(access)
                })?;
                Ok(())
            }
            TableWorkloadInteraction::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.db.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                self.capture_pending_snapshot_if_idle()?;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_commit_or_rollback(access)
                })?;
                Ok(())
            }
            TableWorkloadInteraction::Insert { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    engine
                        .db
                        .insert(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(())
                })?;
                if !in_tx {
                    self.sync_and_snapshot(false)?;
                }
                let step = self.step as u64;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_insert(access, step, *conn, *table, row, in_tx)
                })
            }
            TableWorkloadInteraction::Delete { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, |engine, tx| {
                    let table_id = *engine
                        .base_table_ids
                        .get(*table)
                        .ok_or_else(|| format!("table {table} out of range"))?;
                    let deleted = engine.db.delete_by_rel(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                })?;
                if !in_tx {
                    self.sync_and_snapshot(false)?;
                }
                let step = self.step as u64;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_delete(access, step, *conn, *table, row, in_tx)
                })
            }
        }
    }

    fn with_mut_tx(
        &mut self,
        conn: usize,
        mut f: impl FnMut(&mut Self, &mut RelMutTx) -> Result<(), String>,
    ) -> Result<(), String> {
        self.execution.ensure_known_connection(conn)?;
        if self.execution.tx_by_connection[conn].is_some() {
            let mut tx = self.execution.tx_by_connection[conn]
                .take()
                .ok_or_else(|| format!("connection {conn} missing transaction handle"))?;
            f(self, &mut tx)?;
            self.execution.tx_by_connection[conn] = Some(tx);
            return Ok(());
        }

        if let Some(owner) = self.execution.active_writer {
            return Err(format!(
                "connection {conn} cannot auto-commit write while connection {owner} owns lock"
            ));
        }

        let mut tx = self.db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        self.execution.active_writer = Some(conn);
        f(self, &mut tx)?;
        self.commit_tx_capture(tx, "auto-commit write")?;
        self.execution.active_writer = None;
        self.capture_pending_snapshot_if_idle()?;
        Ok(())
    }

    fn create_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<(), String> {
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "create dynamic table");
        self.with_mut_tx(conn, |engine, tx| {
            if engine.dynamic_tables.contains_key(&slot) {
                return Ok(());
            }
            let name = dynamic_table_name(slot, 0);
            let schema = dynamic_schema(&name, 0);
            let table_id = engine
                .db
                .create_table(tx, schema)
                .map_err(|err| format!("create dynamic table slot={slot} failed: {err}"))?;
            engine
                .dynamic_tables
                .insert(slot, DynamicTableState { version: 0, table_id });
            Ok(())
        })?;
        self.sync_and_snapshot(false)
    }

    fn drop_dynamic_table(&mut self, conn: usize, slot: u32) -> Result<(), String> {
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "drop dynamic table");
        self.with_mut_tx(conn, |engine, tx| {
            let Some(state) = engine.dynamic_tables.remove(&slot) else {
                return Ok(());
            };
            if let Err(err) = engine.db.drop_table(tx, state.table_id) {
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
        let conn = self.normalize_conn(conn);
        debug!(step = self.step, conn, slot, "migrate dynamic table");
        self.with_mut_tx(conn, |engine, tx| {
            let Some(state) = engine.dynamic_tables.get(&slot).cloned() else {
                return Ok(());
            };
            let to_version = state.version.saturating_add(1);
            let to_name = dynamic_table_name(slot, to_version);
            let to_schema = dynamic_schema(&to_name, to_version);
            let new_table_id = engine
                .db
                .create_table(tx, to_schema)
                .map_err(|err| format!("migrate create new table slot={slot} failed: {err}"))?;
            let existing_rows = engine
                .db
                .iter_mut(tx, state.table_id)
                .map_err(|err| format!("migrate scan old table failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            for row in &existing_rows {
                let mut migrated = row.clone();
                if to_version > 0 && migrated.values.len() < 3 {
                    migrated.values.push(AlgebraicValue::Bool(false));
                }
                let bsatn = migrated.to_bsatn().map_err(|err| err.to_string())?;
                engine
                    .db
                    .insert(tx, new_table_id, &bsatn)
                    .map_err(|err| format!("migrate copy row failed: {err}"))?;
            }
            if let Err(err) = engine.db.drop_table(tx, state.table_id) {
                let msg = err.to_string();
                if !msg.contains("not found") {
                    return Err(format!("migrate drop old table slot={slot} failed: {err}"));
                }
            }
            engine.dynamic_tables.insert(
                slot,
                DynamicTableState {
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

    fn commit_tx_capture(&mut self, tx: RelMutTx, context: &str) -> Result<(), String> {
        let committed = self
            .db
            .commit_tx(tx)
            .map_err(|err| format!("{context} commit failed: {err}"))?;
        if let Some((offset, tx_data, _, _)) = committed {
            let Some(encoded) = encode_txdata_for_commitlog(&tx_data) else {
                trace!(step = self.step, context, "commit had no durable payload");
                return Ok(());
            };
            trace!(step = self.step, context, offset, "append tx to mock commitlog");
            self.commitlog
                .append(offset, encoded)
                .map_err(|err| format!("{context} append to mock commitlog failed: {err}"))?;
        }
        Ok(())
    }

    fn sync_and_snapshot(&mut self, forced: bool) -> Result<(), String> {
        let advanced = self
            .commitlog
            .sync(forced)
            .map_err(|err| format!("mock sync failed: {err}"))?;
        trace!(
            step = self.step,
            forced,
            advanced,
            durable_count = self.commitlog.durable_count(),
            "mock sync"
        );
        if advanced {
            if self.execution.active_writer.is_some() {
                self.pending_snapshot_capture = true;
                trace!("defer durable snapshot capture until writer releases");
            } else {
                self.last_durable_snapshot = self.snapshot_tracked_tables()?;
                self.pending_snapshot_capture = false;
                debug!(
                    tables = self.last_durable_snapshot.len(),
                    "captured durable snapshot after sync"
                );
            }
        }
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
                .db
                .iter_by_col_eq_mut(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("in-tx lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next())
        } else {
            let tx = self.db.begin_tx(Workload::ForTests);
            let found = self
                .db
                .iter_by_col_eq(&tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next();
            let _ = self.db.release_tx(tx);
            Ok(found)
        }
    }

    fn count_rows_for_property(&self, table: usize) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        let tx = self.db.begin_tx(Workload::ForTests);
        let total = self
            .db
            .iter(&tx, table_id)
            .map_err(|err| format!("scan failed: {err}"))?
            .count();
        let _ = self.db.release_tx(tx);
        Ok(total)
    }

    fn count_by_col_eq_for_property(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        let tx = self.db.begin_tx(Workload::ForTests);
        let total = self
            .db
            .iter_by_col_eq(&tx, table_id, col, value)
            .map_err(|err| format!("predicate query failed: {err}"))?
            .count();
        let _ = self.db.release_tx(tx);
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
        let tx = self.db.begin_tx(Workload::ForTests);
        let cols = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let rows = self
            .db
            .iter_by_col_range(&tx, table_id, cols, (lower, upper))
            .map_err(|err| format!("range scan failed: {err}"))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
        let _ = self.db.release_tx(tx);
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
        let tx = self.db.begin_tx(Workload::ForTests);
        let mut rows = self
            .db
            .iter(&tx, table_id)
            .map_err(|err| format!("scan failed: {err}"))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
        let _ = self.db.release_tx(tx);
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
        for (slot, state) in &self.dynamic_tables {
            let name = dynamic_table_name(*slot, state.version);
            snap.insert(name, self.collect_rows_by_id(state.table_id)?);
        }
        Ok(snap)
    }

    fn collect_outcome(&mut self) -> Result<RelationalDbCommitlogOutcome, String> {
        self.capture_pending_snapshot_if_idle()?;
        self.sync_and_snapshot(true)?;
        let history = MockHistory::from_durable(self.commitlog.durable_records())?;
        let replayed = reopen_from_history(history)?;
        debug!(
            durable_commits = self.commitlog.durable_count(),
            replay_tables = replayed.len(),
            "replayed durable prefix"
        );
        Ok(RelationalDbCommitlogOutcome {
            applied_steps: self.step,
            durable_commit_count: self.commitlog.durable_count(),
            replay_table_count: replayed.len(),
        })
    }

    fn finish(&mut self) {
        for tx in &mut self.execution.tx_by_connection {
            if let Some(tx) = tx.take() {
                let _ = self.db.rollback_mut_tx(tx);
            }
        }
        self.execution.active_writer = None;
    }
}

impl TargetPropertyAccess for RelationalDbCommitlogEngine {
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

fn reopen_from_history(history: MockHistory) -> Result<DurableSnapshot, String> {
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

fn bootstrap_relational_db() -> anyhow::Result<RelationalDB> {
    let (db, connected_clients) = RelationalDB::open(
        Identity::ZERO,
        Identity::ZERO,
        EmptyHistory::new(),
        None,
        None,
        PagePool::new_for_test(),
    )?;
    assert_eq!(connected_clients.len(), 0);
    Ok(db)
}

fn dynamic_table_name(slot: u32, version: u32) -> String {
    format!("dst_dynamic_slot_{slot}_v{version}")
}

fn dynamic_schema(name: &str, version: u32) -> TableSchema {
    let mut columns = vec![
        ColumnSchema::for_test(0, "id", AlgebraicType::U64),
        ColumnSchema::for_test(1, "value", AlgebraicType::U64),
    ];
    if version > 0 {
        columns.push(ColumnSchema::for_test(2, "migrated", AlgebraicType::Bool));
    }
    let indexes = vec![IndexSchema::for_test(format!("{name}_id_idx"), BTreeAlgorithm::from(0))];
    let constraints = vec![ConstraintSchema::unique_for_test(format!("{name}_id_unique"), 0)];
    TableSchema::new(
        TableId::SENTINEL,
        TableName::for_test(name),
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
    )
}

fn encode_txdata_for_commitlog(tx_data: &DatastoreTxData) -> Option<Txdata> {
    let _tx_offset = tx_data.tx_offset()?;
    let mut inserts: Box<_> = tx_data
        .persistent_inserts()
        .map(|(table_id, rowdata)| commitlog::payload::txdata::Ops { table_id, rowdata })
        .collect();
    inserts.sort_unstable_by_key(|ops| ops.table_id);

    let mut deletes: Box<_> = tx_data
        .persistent_deletes()
        .map(|(table_id, rowdata)| commitlog::payload::txdata::Ops { table_id, rowdata })
        .collect();
    deletes.sort_unstable_by_key(|ops| ops.table_id);

    let mut truncates: Box<[_]> = tx_data.persistent_truncates().collect();
    truncates.sort_unstable_by_key(|table_id| *table_id);

    Some(Txdata {
        inputs: None,
        outputs: None,
        mutations: Some(commitlog::payload::txdata::Mutations {
            inserts,
            deletes,
            truncates,
        }),
    })
}

/// Deterministic mocked file/commitlog layer with chaos.
struct MockCommitlogFs {
    chaos_rng: DstRng,
    pending: Vec<(u64, Txdata)>,
    durable: Vec<(u64, Txdata)>,
    commits_since_sync: usize,
}

impl MockCommitlogFs {
    fn new(seed: DstSeed) -> Self {
        Self {
            chaos_rng: seed.rng(),
            pending: Vec::new(),
            durable: Vec::new(),
            commits_since_sync: 0,
        }
    }

    fn append(&mut self, tx_offset: u64, txdata: Txdata) -> Result<(), String> {
        // deterministic append chaos: low-rate injected write failure
        if self.chaos_rng.index(1000) < 6 {
            warn!(tx_offset, "mock commitlog injected append error");
            return Err("injected append error".to_string());
        }
        if let Some((last_offset, _)) = self.pending.last().or_else(|| self.durable.last())
            && tx_offset != last_offset.saturating_add(1)
        {
            return Err(format!(
                "non-contiguous commitlog append: got={tx_offset} expected={}",
                last_offset.saturating_add(1)
            ));
        }
        self.pending.push((tx_offset, txdata));
        self.commits_since_sync = self.commits_since_sync.saturating_add(1);
        trace!(
            tx_offset,
            pending = self.pending.len(),
            durable = self.durable.len(),
            commits_since_sync = self.commits_since_sync,
            "mock commitlog append"
        );
        Ok(())
    }

    fn sync(&mut self, forced: bool) -> Result<bool, String> {
        if self.pending.is_empty() {
            return Ok(false);
        }

        // periodic delayed fsync behavior
        let should_attempt = forced || self.commits_since_sync >= 3 || self.chaos_rng.index(100) < 30;
        if !should_attempt {
            trace!(
                forced,
                pending = self.pending.len(),
                commits_since_sync = self.commits_since_sync,
                "mock sync skipped (delay)"
            );
            return Ok(false);
        }

        // injected fsync miss: pretend sync happened but keep data pending
        if !forced && self.chaos_rng.index(100) < 12 {
            self.commits_since_sync = 0;
            warn!(
                pending = self.pending.len(),
                "mock sync injected miss (no durable advance)"
            );
            return Ok(false);
        }

        let mut advanced = false;
        for pending in self.pending.drain(..) {
            self.durable.push(pending);
            advanced = true;
        }
        self.commits_since_sync = 0;
        debug!(durable = self.durable.len(), "mock sync advanced durable prefix");
        Ok(advanced)
    }

    fn durable_records(&self) -> &[(u64, Txdata)] {
        &self.durable
    }

    fn durable_count(&self) -> usize {
        self.durable.len()
    }
}

/// In-memory history used to replay exactly the durable commitlog prefix.
struct MockHistory(commitlog::commitlog::Generic<commitlog::repo::Memory, Txdata>);

impl MockHistory {
    fn from_durable(records: &[(u64, Txdata)]) -> Result<Self, String> {
        let mut log = commitlog::commitlog::Generic::open(commitlog::repo::Memory::unlimited(), Default::default())
            .map_err(|err| format!("open in-memory commitlog failed: {err}"))?;
        for (offset, txdata) in records {
            log.commit([(*offset, txdata.clone())])
                .map_err(|err| format!("append durable tx offset={offset} failed: {err}"))?;
        }
        Ok(Self(log))
    }
}

impl History for MockHistory {
    type TxData = Txdata;

    fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
    where
        D: commitlog::Decoder,
        D::Error: From<Traversal>,
    {
        self.0.fold_transactions_from(offset, decoder)
    }

    fn transactions_from<'a, D>(
        &self,
        offset: TxOffset,
        decoder: &'a D,
    ) -> impl Iterator<Item = Result<commitlog::Transaction<Self::TxData>, D::Error>>
    where
        D: commitlog::Decoder<Record = Self::TxData>,
        D::Error: From<Traversal>,
        Self::TxData: 'a,
    {
        self.0.transactions_from(offset, decoder)
    }

    fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
        let min = self.0.min_committed_offset().unwrap_or_default();
        let max = self.0.max_committed_offset();
        (min, max)
    }
}
