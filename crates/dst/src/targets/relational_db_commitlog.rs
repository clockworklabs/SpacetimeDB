//! Simple RelationalDB DST target — table operations only.

use std::ops::Bound;
use std::sync::Arc;

use spacetimedb_commitlog::repo::mem::Memory;
use spacetimedb_core::{
    db::persistence::{DiskSizeFn, Persistence},
    db::relational_db::{MutTx as RelMutTx, RelationalDB, Tx as RelTx},
    error::DBError,
    messages::control_db::HostType,
};
use spacetimedb_datastore::{execution_context::Workload, traits::IsolationLevel};
use spacetimedb_durability::local::Options as DurabilityOpts;
use spacetimedb_durability::Local as DurabilityLocal;
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::TableId;
use spacetimedb_runtime::Handle as RuntimeHandle;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_snapshot::SnapshotStore;
use spacetimedb_table::page_pool::PagePool;
use tracing::{info, trace};

use crate::{
    client::SessionId,
    config::{CommitlogFaultProfile, RunConfig},
    core::{self, TargetEngine},
    properties::{
        PropertyRuntime, TableMutation, TableObservation, TargetPropertyAccess,
    },
    schema::{SchemaPlan, SimRow},
    sim::{
        commitlog::{CommitlogFaultConfig, FaultableRepo},
        fork_seed,
        snapshot::BuggifiedSnapshotRepo,
        storage_faults::StorageFaultConfig,
        Rng,
    },
    workload::table_ops::{
        ConnectionWriteState, TableErrorKind, TableOperation, TableScenario, TableScenarioId, TableWorkloadInteraction,
        TableWorkloadOutcome, TableWorkloadSource,
    },
};

pub type RelationalDbTableOutcome = TableWorkloadOutcome;

pub async fn run_generated_with_config_and_scenario(
    seed: u64,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<RelationalDbTableOutcome> {
    let num_connections = {
        let rng = Rng::new(fork_seed(seed, 121));
        rng.index(3) + 1
    };
    let schema_rng = Rng::new(fork_seed(seed, 122));
    let schema = scenario.generate_schema(&schema_rng);
    let source = TableWorkloadSource::new(
        seed,
        scenario,
        schema.clone(),
        num_connections,
        config.max_interactions_or_default(usize::MAX),
    );

    let sim_handle = crate::sim::current_handle().expect("must run inside sim Runtime::block_on");
    let rt_handle = RuntimeHandle::simulation(sim_handle.clone());

    // Build faulty commitlog + persistence
    let clog_repo = FaultableRepo::new(
        Memory::unlimited(),
        CommitlogFaultConfig::for_profile(CommitlogFaultProfile::Default),
    );
    let local = DurabilityLocal::open_with_repo(clog_repo, rt_handle.clone(), DurabilityOpts::default())?;
    let history = local.as_history();
    let durability = Arc::new(local);

    // Build faulty snapshot store
    let snap_repo = Arc::new(BuggifiedSnapshotRepo::new(
        StorageFaultConfig::for_profile(CommitlogFaultProfile::Default),
    )?) as Arc<dyn SnapshotStore>;

    // Enable buggify after setup so initial replay is fault-free
    sim_handle.enable_buggify();

    let persistence = Persistence {
        durability,
        disk_size: {
            use std::io;
            use spacetimedb_commitlog::repo::SizeOnDisk;
            Arc::new(|| io::Result::Ok(SizeOnDisk { total_bytes: 0, total_blocks: 0 })) as DiskSizeFn
        },
        snapshot_store: Some(snap_repo),
        snapshots: None,
        runtime: rt_handle,
    };

    let engine = RelationalDbEngine::new(seed, &schema, num_connections, history, Some(persistence))?;
    let properties = PropertyRuntime::for_table_workload(scenario, schema.clone(), num_connections);
    let outcome = core::run_streaming(source, engine, properties, config).await?;
    info!(
        applied_steps = outcome.final_row_counts.iter().sum::<u64>(),
        "relational_db_table complete"
    );
    Ok(outcome)
}

struct RelationalDbEngine {
    db: Option<RelationalDB>,
    execution: ConnectionWriteState<RelMutTx>,
    read_tx_by_connection: Vec<Option<RelTx>>,
    base_schema: SchemaPlan,
    base_table_ids: Vec<TableId>,
    step: usize,
}

impl RelationalDbEngine {
    fn new<H: spacetimedb_durability::History<TxData = spacetimedb_core::db::relational_db::Txdata>>(
        _seed: u64, schema: &SchemaPlan, num_connections: usize,
        history: H, persistence: Option<Persistence>,
    ) -> anyhow::Result<Self> {
        let (db, connected_clients) = RelationalDB::open(
            Identity::ZERO,
            Identity::ZERO,
            history,
            persistence,
            None,
            PagePool::new_for_test(),
        )?;
        assert_eq!(connected_clients.len(), 0);
        db.with_auto_commit(Workload::Internal, |tx| {
            db.set_initialized(tx, spacetimedb_datastore::traits::Program::empty(HostType::Wasm.into()))
        })?;

        let mut engine = Self {
            db: Some(db),
            execution: ConnectionWriteState::new(num_connections),
            read_tx_by_connection: (0..num_connections).map(|_| None).collect(),
            base_schema: schema.clone(),
            base_table_ids: Vec::with_capacity(schema.tables.len()),
            step: 0,
        };
        engine.install_base_schema().map_err(anyhow::Error::msg)?;
        Ok(engine)
    }

    fn db(&self) -> Result<&RelationalDB, String> {
        self.db.as_ref().ok_or_else(|| "relational db not initialized".to_string())
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
        let _ = self
            .db()?
            .commit_tx(tx)
            .map_err(|err| format!("install base schema commit failed: {err}"))?;
        Ok(())
    }

    fn execute(&mut self, interaction: &TableWorkloadInteraction) -> Result<TableObservation, String> {
        self.step = self.step.saturating_add(1);
        self.execute_table_op(interaction)
    }

    fn execute_table_op(&mut self, interaction: &TableWorkloadInteraction) -> Result<TableObservation, String> {
        trace!(step = self.step, op = ?interaction.op, "table interaction");
        let observation = self.execute_table_op_inner(&interaction.op)?;
        Ok(observation)
    }

    fn execute_table_op_inner(&mut self, op: &TableOperation) -> Result<TableObservation, String> {
        match op {
            TableOperation::BeginTx { conn } => self.begin_write_tx(*conn),
            TableOperation::BeginReadTx { conn } => {
                self.execution.ensure_known_connection(*conn)?;
                if self.execution.tx_by_connection[conn.as_index()].is_some() {
                    return Err(format!("connection {conn} already has open write transaction"));
                }
                if self.read_tx_by_connection[conn.as_index()].is_some() {
                    return Err(format!("connection {conn} already has open read transaction"));
                }
                let tx = self.db()?.begin_tx(Workload::ForTests);
                self.read_tx_by_connection[conn.as_index()] = Some(tx);
                Ok(TableObservation::Applied)
            }
            TableOperation::ReleaseReadTx { conn } => {
                self.execution.ensure_known_connection(*conn)?;
                let tx = self.read_tx_by_connection[conn.as_index()]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no read transaction to release"))?;
                let _ = self.db()?.release_tx(tx);
                Ok(TableObservation::Applied)
            }
            TableOperation::CommitTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "commit")?;
                let tx = self.execution.tx_by_connection[conn.as_index()]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
                let _ = self
                    .db()?
                    .commit_tx(tx)
                    .map_err(|err| format!("commit interaction failed: {err}"))?;
                self.execution.active_writer = None;
                Ok(TableObservation::CommitOrRollback)
            }
            TableOperation::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[conn.as_index()]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.db()?.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                Ok(TableObservation::CommitOrRollback)
            }
            TableOperation::InsertRows { conn, table, rows } => self.execute_insert_rows(*conn, *table, rows),
            TableOperation::DeleteRows { conn, table, rows } => self.execute_delete_rows(*conn, *table, rows),
            TableOperation::AddColumn {
                conn,
                table,
                column,
                default,
            } => {
                let table_id = self.table_id_for_index(*table)?;
                let column_idx = self.base_schema.tables[*table].columns.len() as u16;
                let mut columns = self.base_schema.tables[*table]
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(idx, existing)| ColumnSchema::for_test(idx as u16, &existing.name, existing.ty.clone()))
                    .collect::<Vec<_>>();
                columns.push(ColumnSchema::for_test(column_idx, &column.name, column.ty.clone()));
                self.with_mut_tx(*conn, |engine, tx| {
                    let new_table_id = engine
                        .db()?
                        .add_columns_to_table(tx, table_id, columns.clone(), vec![default.clone()])
                        .map_err(|err| format!("add column failed: {err}"))?;
                    Ok(new_table_id)
                })?;
                Ok(TableObservation::Applied)
            }
            TableOperation::AddIndex { conn, table, cols } => {
                let table_id = self.table_id_for_index(*table)?;
                self.with_mut_tx(*conn, |engine, tx| {
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

    fn begin_write_tx(&mut self, conn: SessionId) -> Result<TableObservation, String> {
        self.execution.ensure_known_connection(conn)?;
        if self.read_tx_by_connection[conn.as_index()].is_some() {
            return Err(format!("connection {conn} already has open read transaction"));
        }
        if self.execution.tx_by_connection[conn.as_index()].is_some() {
            return Err(format!("connection {conn} already has open transaction"));
        }
        match self
            .db()?
            .try_begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests)
        {
            Some(tx) => {
                if self.execution.active_writer.is_some() || self.any_open_read_tx() {
                    let _ = self.db()?.rollback_mut_tx(tx);
                    return Err(format!(
                        "connection {conn} unexpectedly acquired write lock while conflicting transaction was open"
                    ));
                }
                self.execution.tx_by_connection[conn.as_index()] = Some(tx);
                self.execution.active_writer = Some(conn);
                Ok(TableObservation::Applied)
            }
            None => {
                if self.execution.active_writer.is_some() || self.any_open_read_tx() {
                    Ok(TableObservation::ObservedError(
                        TableErrorKind::WriteConflict,
                    ))
                } else {
                    Err(format!(
                        "connection {conn} failed to begin write transaction without an open conflicting lock"
                    ))
                }
            }
        }
    }

    fn execute_insert_rows(
        &mut self,
        conn: SessionId,
        table: usize,
        rows: &[SimRow],
    ) -> Result<TableObservation, String> {
        let in_tx = self.is_in_write_tx(conn);
        let outcome = self.with_mut_tx_observed(conn, |engine, tx| {
            let mut mutations = Vec::with_capacity(rows.len());
            for row in rows {
                match engine.try_insert_base_row(tx, table, row)? {
                    Ok(returned) => mutations.push(TableMutation::Inserted {
                        table,
                        requested: row.clone(),
                        returned,
                    }),
                    Err(err) if is_unique_constraint_violation(&err) => {
                        return Ok(Err(TableErrorKind::UniqueConstraintViolation));
                    }
                    Err(err) => return Err(format!("insert failed: {err}")),
                }
            }
            Ok(Ok(mutations))
        });
        self.mutation_observation(conn, in_tx, outcome)
    }

    fn execute_delete_rows(
        &mut self,
        conn: SessionId,
        table: usize,
        rows: &[SimRow],
    ) -> Result<TableObservation, String> {
        let in_tx = self.is_in_write_tx(conn);
        let outcome = self.with_mut_tx_observed(conn, |engine, tx| {
            let mut mutations = Vec::with_capacity(rows.len());
            for row in rows {
                match engine.delete_base_row_count(tx, table, row)? {
                    0 => return Ok(Err(TableErrorKind::MissingRow)),
                    1 => mutations.push(TableMutation::Deleted {
                        table,
                        row: row.clone(),
                    }),
                    deleted => {
                        return Err(format!("delete for row={row:?} affected {deleted} rows"));
                    }
                }
            }
            Ok(Ok(mutations))
        });
        self.mutation_observation(conn, in_tx, outcome)
    }

    fn mutation_observation(
        &mut self,
        conn: SessionId,
        in_tx: bool,
        outcome: Result<Result<Vec<TableMutation>, TableErrorKind>, String>,
    ) -> Result<TableObservation, String> {
        match outcome {
            Ok(Ok(mutations)) => Ok(TableObservation::Mutated { conn, mutations, in_tx }),
            Ok(Err(kind)) => Ok(TableObservation::ObservedError(kind)),
            Err(err) if is_write_conflict_error(&err) => {
                Ok(TableObservation::ObservedError(TableErrorKind::WriteConflict))
            }
            Err(err) => Err(err),
        }
    }

    fn with_mut_tx_observed<T>(
        &mut self,
        conn: SessionId,
        mut f: impl FnMut(&mut Self, &mut RelMutTx) -> Result<Result<T, TableErrorKind>, String>,
    ) -> Result<Result<T, TableErrorKind>, String> {
        self.execution.ensure_known_connection(conn)?;
        if self.read_tx_by_connection[conn.as_index()].is_some() {
            return Err(format!("connection {conn} cannot write while read transaction is open"));
        }
        if self.execution.tx_by_connection[conn.as_index()].is_some() {
            let mut tx = self.execution.tx_by_connection[conn.as_index()]
                .take()
                .ok_or_else(|| format!("connection {conn} missing transaction handle"))?;
            let result = f(self, &mut tx);
            self.execution.tx_by_connection[conn.as_index()] = Some(tx);
            return result;
        }

        if self.execution.active_writer.is_some() || self.any_open_read_tx() {
            return Ok(Err(TableErrorKind::WriteConflict));
        }

        let mut tx = self
            .db()?
            .try_begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests)
            .ok_or_else(|| format!("connection {conn} failed to acquire write transaction"))?;
        self.execution.active_writer = Some(conn);
        let value = match f(self, &mut tx) {
            Ok(Ok(value)) => value,
            Ok(Err(kind)) => {
                let _ = self.db()?.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                return Ok(Err(kind));
            }
            Err(err) => {
                let _ = self.db()?.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                return Err(err);
            }
        };
        let _ = self
            .db()?
            .commit_tx(tx)
            .map_err(|err| format!("auto-commit write failed: {err}"))?;
        self.execution.active_writer = None;
        Ok(Ok(value))
    }

    fn with_mut_tx<T>(
        &mut self,
        conn: SessionId,
        mut f: impl FnMut(&mut Self, &mut RelMutTx) -> Result<T, String>,
    ) -> Result<T, String> {
        self.execution.ensure_known_connection(conn)?;
        if self.read_tx_by_connection[conn.as_index()].is_some() {
            return Err(format!("connection {conn} cannot write while read transaction is open"));
        }
        if self.execution.tx_by_connection[conn.as_index()].is_some() {
            let mut tx = self.execution.tx_by_connection[conn.as_index()]
                .take()
                .ok_or_else(|| format!("connection {conn} missing transaction handle"))?;
            let result = f(self, &mut tx);
            self.execution.tx_by_connection[conn.as_index()] = Some(tx);
            return result;
        }

        if self.execution.active_writer.is_some() || self.any_open_read_tx() {
            return Err(format!(
                "connection {conn} cannot auto-commit write while a conflicting lock is open"
            ));
        }

        let mut tx = self
            .db()?
            .try_begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests)
            .ok_or_else(|| format!("connection {conn} failed to acquire write transaction"))?;
        self.execution.active_writer = Some(conn);
        let value = match f(self, &mut tx) {
            Ok(value) => value,
            Err(err) => {
                let _ = self.db()?.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                return Err(err);
            }
        };
        let _ = self
            .db()?
            .commit_tx(tx)
            .map_err(|err| format!("auto-commit write failed: {err}"))?;
        self.execution.active_writer = None;
        Ok(value)
    }

    fn try_insert_base_row(
        &self,
        tx: &mut RelMutTx,
        table: usize,
        row: &SimRow,
    ) -> Result<Result<SimRow, DBError>, String> {
        let table_id = self.table_id_for_index(table)?;
        let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
        Ok(match self.db()?.insert(tx, table_id, &bsatn) {
            Ok((_, row_ref, _)) => Ok(SimRow::from_product_value(row_ref.to_product_value())),
            Err(err) => Err(err),
        })
    }

    fn delete_base_row_count(&self, tx: &mut RelMutTx, table: usize, row: &SimRow) -> Result<u32, String> {
        let table_id = self.table_id_for_index(table)?;
        Ok(self.db()?.delete_by_rel(tx, table_id, [row.to_product_value()]))
    }

    fn any_open_read_tx(&self) -> bool {
        self.read_tx_by_connection.iter().any(Option::is_some)
    }

    fn is_in_write_tx(&self, conn: SessionId) -> bool {
        self.execution
            .tx_by_connection
            .get(conn.as_index())
            .is_some_and(Option::is_some)
    }

    fn table_id_for_index(&self, table: usize) -> Result<TableId, String> {
        self.base_table_ids
            .get(table)
            .copied()
            .ok_or_else(|| format!("table {table} out of range"))
    }

    fn with_fresh_read_tx<T>(&self, f: impl FnOnce(&RelationalDB, &RelTx) -> Result<T, String>) -> Result<T, String> {
        let db = self.db()?;
        let tx = db.begin_tx(Workload::ForTests);
        let result = f(db, &tx);
        let _ = db.release_tx(tx);
        result
    }

    fn collect_rows_by_id(&self, table_id: TableId) -> Result<Vec<SimRow>, String> {
        self.with_fresh_read_tx(|db, tx| {
            let mut rows = db
                .iter(tx, table_id)
                .map_err(|err| format!("scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        })
    }

    fn lookup_base_row(&self, conn: SessionId, table: usize, id: u64) -> Result<Option<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn.as_index()) {
            Ok(self
                .db()?
                .iter_by_col_eq_mut(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("in-tx lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next())
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn.as_index()) {
            Ok(self
                .db()?
                .iter_by_col_eq(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("read-tx lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next())
        } else {
            self.with_fresh_read_tx(|db, tx| {
                Ok(db
                    .iter_by_col_eq(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                    .map_err(|err| format!("lookup failed: {err}"))?
                    .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                    .next())
            })
        }
    }

    fn collect_rows_in_connection(&self, conn: SessionId, table: usize) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn.as_index()) {
            let mut rows = self
                .db()?
                .iter_mut(tx, table_id)
                .map_err(|err| format!("in-tx scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn.as_index()) {
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
        conn: SessionId,
        table: usize,
        col: u16,
        value: &AlgebraicValue,
    ) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn.as_index()) {
            Ok(self
                .db()?
                .iter_by_col_eq_mut(tx, table_id, col, value)
                .map_err(|err| format!("in-tx predicate query failed: {err}"))?
                .count())
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn.as_index()) {
            Ok(self
                .db()?
                .iter_by_col_eq(tx, table_id, col, value)
                .map_err(|err| format!("read-tx predicate query failed: {err}"))?
                .count())
        } else {
            self.with_fresh_read_tx(|db, tx| {
                Ok(db
                    .iter_by_col_eq(tx, table_id, col, value)
                    .map_err(|err| format!("predicate query failed: {err}"))?
                    .count())
            })
        }
    }

    fn range_scan_in_connection(
        &self,
        conn: SessionId,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        let cols_list = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn.as_index()) {
            let mut rows = self
                .db()?
                .iter_by_col_range_mut(tx, table_id, cols_list, (lower, upper))
                .map_err(|err| format!("in-tx range scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        } else if let Some(Some(tx)) = self.read_tx_by_connection.get(conn.as_index()) {
            let mut rows = self
                .db()?
                .iter_by_col_range(tx, table_id, cols_list, (lower, upper))
                .map_err(|err| format!("read-tx range scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        } else {
            self.with_fresh_read_tx(|db, tx| {
                let mut rows = db
                    .iter_by_col_range(tx, table_id, cols_list, (lower, upper))
                    .map_err(|err| format!("range scan failed: {err}"))?
                    .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                    .collect::<Vec<_>>();
                rows.sort_by_key(|row| row.id().unwrap_or_default());
                Ok(rows)
            })
        }
    }
}

impl TargetEngine<TableWorkloadInteraction> for RelationalDbEngine {
    type Observation = TableObservation;
    type Outcome = TableWorkloadOutcome;
    type Error = String;

    fn execute_interaction<'a>(
        &'a mut self,
        interaction: &'a TableWorkloadInteraction,
    ) -> impl std::future::Future<Output = Result<Self::Observation, Self::Error>> + 'a {
        async move { self.execute(interaction) }
    }

    fn finish(&mut self) {}

    fn collect_outcome<'a>(&'a mut self) -> impl std::future::Future<Output = anyhow::Result<Self::Outcome>> + 'a {
        async move {
            let mut final_rows = Vec::with_capacity(self.base_schema.tables.len());
            let mut final_row_counts = Vec::with_capacity(self.base_schema.tables.len());
            for table in 0..self.base_schema.tables.len() {
                let table_id = self.table_id_for_index(table).map_err(anyhow::Error::msg)?;
                let rows = self.collect_rows_by_id(table_id).map_err(anyhow::Error::msg)?;
                final_row_counts.push(rows.len() as u64);
                final_rows.push(rows);
            }
            Ok(TableWorkloadOutcome {
                final_row_counts,
                final_rows,
            })
        }
    }
}

impl TargetPropertyAccess for RelationalDbEngine {
    fn schema_plan(&self) -> &SchemaPlan {
        &self.base_schema
    }

    fn lookup_in_connection(&self, conn: SessionId, table: usize, id: u64) -> Result<Option<SimRow>, String> {
        self.lookup_base_row(conn, table, id)
    }

    fn collect_rows_in_connection(&self, conn: SessionId, table: usize) -> Result<Vec<SimRow>, String> {
        self.collect_rows_in_connection(conn, table)
    }

    fn collect_rows_for_table(&self, table: usize) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        self.collect_rows_by_id(table_id)
    }

    fn count_rows(&self, table: usize) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        self.with_fresh_read_tx(|db, tx| {
            Ok(db
                .iter(tx, table_id)
                .map_err(|err| format!("count rows failed: {err}"))?
                .count())
        })
    }

    fn count_by_col_eq(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String> {
        let table_id = self.table_id_for_index(table)?;
        self.with_fresh_read_tx(|db, tx| {
            Ok(db
                .iter_by_col_eq(tx, table_id, col, value)
                .map_err(|err| format!("count by col eq failed: {err}"))?
                .count())
        })
    }

    fn range_scan(
        &self,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String> {
        let table_id = self.table_id_for_index(table)?;
        let cols_list = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        self.with_fresh_read_tx(|db, tx| {
            let mut rows = db
                .iter_by_col_range(tx, table_id, cols_list, (lower, upper))
                .map_err(|err| format!("range scan failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            Ok(rows)
        })
    }
}

fn is_unique_constraint_violation(err: &DBError) -> bool {
    err.to_string().contains("Unique") || err.to_string().contains("unique")
}

fn is_write_conflict_error(err: &str) -> bool {
    err.contains("WriteConflict") || err.contains("write conflict") || err.contains("Serialization failure")
}
