//! Basic RelationalDB simulator target using the shared table workload.

use std::ops::Bound;

use spacetimedb_core::{
    db::relational_db::{MutTx as RelMutTx, RelationalDB},
    messages::control_db::HostType,
};
use spacetimedb_datastore::{
    execution_context::Workload,
    traits::{IsolationLevel, Program},
};
use spacetimedb_durability::EmptyHistory;
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;

use crate::{
    config::RunConfig,
    schema::{SchemaPlan, SimRow},
    seed::DstSeed,
    targets::{
        harness::{self, TableTargetHarness},
        properties::{PropertyRuntime, TargetPropertyAccess},
    },
    workload::table_ops::{
        ConnectionWriteState, TableScenarioId, TableWorkloadEngine, TableWorkloadInteraction, TableWorkloadOutcome,
    },
};

pub type RelationalDbSimulatorOutcome = TableWorkloadOutcome;
type RelationalDbInteraction = TableWorkloadInteraction;

struct RelationalDbTarget;

impl TableTargetHarness for RelationalDbTarget {
    type Engine = RelationalDbEngine;

    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine> {
        RelationalDbEngine::new(schema, num_connections)
    }
}

pub fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<RelationalDbSimulatorOutcome> {
    harness::run_generated_with_config_and_scenario::<RelationalDbTarget>(seed, scenario, config)
}

/// Concrete `RelationalDB` execution harness for the shared table workload.
struct RelationalDbEngine {
    schema: SchemaPlan,
    db: RelationalDB,
    table_ids: Vec<TableId>,
    execution: ConnectionWriteState<RelMutTx>,
    properties: PropertyRuntime,
    step: u64,
}

impl RelationalDbEngine {
    fn new(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self> {
        let db = bootstrap_relational_db()?;
        let table_ids = install_schema(&db, schema)?;
        Ok(Self {
            schema: schema.clone(),
            db,
            table_ids,
            execution: ConnectionWriteState::new(num_connections),
            properties: PropertyRuntime::default(),
            step: 0,
        })
    }

    fn with_mut_tx(
        &mut self,
        conn: usize,
        table: usize,
        mut f: impl FnMut(&RelationalDB, TableId, &mut RelMutTx) -> Result<(), String>,
    ) -> Result<(), String> {
        let table_id = *self
            .table_ids
            .get(table)
            .ok_or_else(|| format!("table {table} out of range"))?;
        self.execution.ensure_known_connection(conn)?;
        let slot = &mut self.execution.tx_by_connection[conn];

        match slot {
            Some(tx) => f(&self.db, table_id, tx),
            None => {
                if let Some(owner) = self.execution.active_writer {
                    return Err(format!(
                        "connection {conn} cannot auto-commit write while connection {owner} owns lock"
                    ));
                }
                let mut tx = self.db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
                self.execution.active_writer = Some(conn);
                f(&self.db, table_id, &mut tx)?;
                self.db
                    .commit_tx(tx)
                    .map_err(|err| format!("auto-commit failed on connection {conn}: {err}"))?;
                self.execution.active_writer = None;
                Ok(())
            }
        }
    }

    fn fresh_lookup(&self, table_id: TableId, id: u64) -> anyhow::Result<Option<SimRow>> {
        let tx = self.db.begin_tx(Workload::ForTests);
        let result = self
            .db
            .iter_by_col_eq(&tx, table_id, 0u16, &AlgebraicValue::U64(id))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .find(|row| row.id() == Some(id));
        let _ = self.db.release_tx(tx);
        Ok(result)
    }

    fn collect_rows_for_table(&self, table: usize) -> anyhow::Result<Vec<SimRow>> {
        let table_id = *self
            .table_ids
            .get(table)
            .ok_or_else(|| anyhow::anyhow!("table {table} out of range"))?;
        let tx = self.db.begin_tx(Workload::ForTests);
        let mut rows = self
            .db
            .iter(&tx, table_id)?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
        let _ = self.db.release_tx(tx);
        rows.sort_by_key(|row| row.id().unwrap_or_default());
        Ok(rows)
    }

    fn fresh_range_scan(
        &self,
        table_id: TableId,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> anyhow::Result<Vec<SimRow>> {
        let tx = self.db.begin_tx(Workload::ForTests);
        let cols = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let rows = self
            .db
            .iter_by_col_range(&tx, table_id, cols, (lower, upper))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect();
        let _ = self.db.release_tx(tx);
        Ok(rows)
    }

    fn table_id(&self, table: usize) -> Result<TableId, String> {
        self.table_ids
            .get(table)
            .copied()
            .ok_or_else(|| format!("table {table} out of range"))
    }

    fn lookup_in_connection(&self, conn: usize, table: usize, id: u64) -> Result<Option<SimRow>, String> {
        let table_id = self.table_id(table)?;
        if let Some(Some(tx)) = self.execution.tx_by_connection.get(conn) {
            Ok(self
                .db
                .iter_by_col_eq_mut(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                .map_err(|err| format!("in-tx lookup failed: {err}"))?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .next())
        } else {
            self.fresh_lookup(table_id, id)
                .map_err(|err| format!("fresh lookup failed: {err}"))
        }
    }

    fn count_rows_for_property(&self, table: usize) -> Result<usize, String> {
        let table_id = self.table_id(table)?;
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
        let table_id = self.table_id(table)?;
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
        let table_id = self.table_id(table)?;
        self.fresh_range_scan(table_id, cols, lower, upper)
            .map_err(|err| format!("range scan failed: {err}"))
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
}

impl TargetPropertyAccess for RelationalDbEngine {
    fn schema_plan(&self) -> &SchemaPlan {
        &self.schema
    }

    fn lookup_in_connection(&self, conn: usize, table: usize, id: u64) -> Result<Option<SimRow>, String> {
        Self::lookup_in_connection(self, conn, table, id)
    }

    fn collect_rows_for_table(&self, table: usize) -> Result<Vec<SimRow>, String> {
        Self::collect_rows_for_table(self, table).map_err(|err| format!("collect rows failed: {err}"))
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

impl TableWorkloadEngine for RelationalDbEngine {
    fn execute(&mut self, interaction: &RelationalDbInteraction) -> Result<(), String> {
        self.step = self.step.saturating_add(1);
        match interaction {
            RelationalDbInteraction::BeginTx { conn } => {
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
            }
            RelationalDbInteraction::CommitTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "commit")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
                self.db
                    .commit_tx(tx)
                    .map_err(|err| format!("commit failed on connection {conn}: {err}"))?;
                self.execution.active_writer = None;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_commit_or_rollback(access)
                })?;
            }
            RelationalDbInteraction::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.db.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_commit_or_rollback(access)
                })?;
            }
            RelationalDbInteraction::Insert { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, *table, |db, table_id, tx| {
                    let bsatn = row.to_bsatn().map_err(|err: anyhow::Error| err.to_string())?;
                    db.insert(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(())
                })?;
                let step = self.step;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_insert(access, step, *conn, *table, row, in_tx)
                })?;
            }
            RelationalDbInteraction::Delete { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, *table, |db, table_id, tx| {
                    let deleted = db.delete_by_rel(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                })?;
                let step = self.step;
                self.with_property_runtime(|runtime, access| {
                    runtime.on_delete(access, step, *conn, *table, row, in_tx)
                })?;
            }
        }

        Ok(())
    }

    fn collect_outcome(&mut self) -> anyhow::Result<RelationalDbSimulatorOutcome> {
        let tx = self.db.begin_tx(Workload::ForTests);
        let mut final_rows = Vec::with_capacity(self.table_ids.len());
        let mut final_row_counts = Vec::with_capacity(self.table_ids.len());

        for &table_id in &self.table_ids {
            let mut rows = self
                .db
                .iter(&tx, table_id)?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            final_row_counts.push(rows.len() as u64);
            final_rows.push(rows);
        }
        let _ = self.db.release_tx(tx);

        Ok(RelationalDbSimulatorOutcome {
            final_row_counts,
            final_rows,
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
    db.with_auto_commit(Workload::Internal, |tx| {
        db.set_initialized(tx, Program::empty(HostType::Wasm.into()))
    })?;
    Ok(db)
}

fn install_schema(db: &RelationalDB, schema: &SchemaPlan) -> anyhow::Result<Vec<TableId>> {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
    let mut table_ids = Vec::with_capacity(schema.tables.len());

    for table in &schema.tables {
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

        let table_id = db.create_table(
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
        )?;
        table_ids.push(table_id);
    }

    db.commit_tx(tx)?;
    Ok(table_ids)
}
