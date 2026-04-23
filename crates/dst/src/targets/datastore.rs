//! Randomized datastore simulator target built on the shared table workload.

use std::ops::Bound;

use spacetimedb_datastore::{
    execution_context::Workload,
    locking_tx_datastore::{datastore::Locking, MutTxId},
    traits::{IsolationLevel, MutTx, MutTxDatastore, Tx, TxDatastore},
};
use spacetimedb_execution::Datastore as _;
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
        properties::{self, TargetPropertyAccess, TargetPropertyState},
    },
    workload::table_ops::{
        ConnectionWriteState, TableScenarioId, TableWorkloadEngine, TableWorkloadInteraction, TableWorkloadOutcome,
    },
};

pub type DatastoreSimulatorOutcome = TableWorkloadOutcome;
type Interaction = TableWorkloadInteraction;

struct DatastoreTarget;

impl TableTargetHarness for DatastoreTarget {
    type Engine = DatastoreEngine;

    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine> {
        DatastoreEngine::new(schema, num_connections)
    }
}

pub fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<DatastoreSimulatorOutcome> {
    harness::run_generated_with_config_and_scenario::<DatastoreTarget>(seed, scenario, config)
}

/// Concrete datastore execution harness for the shared table workload.
struct DatastoreEngine {
    schema: SchemaPlan,
    datastore: Locking,
    table_ids: Vec<TableId>,
    execution: ConnectionWriteState<MutTxId>,
    properties: TargetPropertyState,
    step: u64,
}

impl DatastoreEngine {
    fn new(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self> {
        let datastore = bootstrap_datastore()?;
        let table_ids = install_schema(&datastore, schema)?;
        Ok(Self {
            schema: schema.clone(),
            datastore,
            table_ids,
            execution: ConnectionWriteState::new(num_connections),
            properties: TargetPropertyState::default(),
            step: 0,
        })
    }

    fn with_mut_tx(
        &mut self,
        conn: usize,
        table: usize,
        mut f: impl FnMut(&Locking, TableId, &mut MutTxId) -> Result<(), String>,
    ) -> Result<(), String> {
        let table_id = *self
            .table_ids
            .get(table)
            .ok_or_else(|| format!("table {table} out of range"))?;
        self.execution.ensure_known_connection(conn)?;
        let slot = &mut self.execution.tx_by_connection[conn];

        match slot {
            Some(tx) => f(&self.datastore, table_id, tx),
            None => {
                if let Some(owner) = self.execution.active_writer {
                    return Err(format!(
                        "connection {conn} cannot auto-commit write while connection {owner} owns lock"
                    ));
                }
                let mut tx = self
                    .datastore
                    .begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
                self.execution.active_writer = Some(conn);
                f(&self.datastore, table_id, &mut tx)?;
                self.datastore
                    .commit_mut_tx(tx)
                    .map_err(|err| format!("auto-commit failed on connection {conn}: {err}"))?;
                self.execution.active_writer = None;
                Ok(())
            }
        }
    }

    fn fresh_lookup(&self, table_id: TableId, id: u64) -> anyhow::Result<Option<SimRow>> {
        let tx = self.datastore.begin_tx(Workload::ForTests);
        Ok(tx
            .table_scan(table_id)?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .find(|row| row.id() == Some(id)))
    }

    fn collect_rows_for_table(&self, table: usize) -> anyhow::Result<Vec<SimRow>> {
        let table_id = *self
            .table_ids
            .get(table)
            .ok_or_else(|| anyhow::anyhow!("table {table} out of range"))?;
        let tx = self.datastore.begin_tx(Workload::ForTests);
        let mut rows = tx
            .table_scan(table_id)?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect::<Vec<_>>();
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
        let tx = self.datastore.begin_tx(Workload::ForTests);
        let cols = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let rows = self
            .datastore
            .iter_by_col_range_tx(&tx, table_id, cols, (lower, upper))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect();
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
                .datastore
                .iter_by_col_eq_mut_tx(tx, table_id, 0u16, &AlgebraicValue::U64(id))
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
        let tx = self.datastore.begin_tx(Workload::ForTests);
        Ok(tx.row_count(table_id) as usize)
    }

    fn count_by_col_eq_for_property(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String> {
        let table_id = self.table_id(table)?;
        let tx = self.datastore.begin_tx(Workload::ForTests);
        self.datastore
            .iter_by_col_eq_tx(&tx, table_id, col, value)
            .map(|rows| rows.count())
            .map_err(|err| format!("predicate query failed: {err}"))
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

    fn with_property_state<T>(
        &mut self,
        f: impl FnOnce(&TargetPropertyState, &Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let state = std::mem::take(&mut self.properties);
        let result = f(&state, self);
        self.properties = state;
        result
    }
}

impl TargetPropertyAccess for DatastoreEngine {
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

impl TableWorkloadEngine for DatastoreEngine {
    fn execute(&mut self, interaction: &Interaction) -> Result<(), String> {
        self.step = self.step.saturating_add(1);
        match interaction {
            Interaction::BeginTx { conn } => {
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
                    self.datastore
                        .begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests),
                );
                self.execution.active_writer = Some(*conn);
            }
            Interaction::CommitTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "commit")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
                self.datastore
                    .commit_mut_tx(tx)
                    .map_err(|err| format!("commit failed on connection {conn}: {err}"))?;
                self.execution.active_writer = None;
                self.with_property_state(|state, access| properties::on_commit_or_rollback(state, access))?;
            }
            Interaction::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.datastore.rollback_mut_tx(tx);
                self.execution.active_writer = None;
                self.with_property_state(|state, access| properties::on_commit_or_rollback(state, access))?;
            }
            Interaction::Insert { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, *table, |datastore, table_id, tx| {
                    let bsatn = row.to_bsatn().map_err(|err: anyhow::Error| err.to_string())?;
                    datastore
                        .insert_mut_tx(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(())
                })?;
                let step = self.step;
                self.with_property_state(|state, access| {
                    properties::on_insert(state, access, step, *conn, *table, row, in_tx)
                })?;
            }
            Interaction::Delete { conn, table, row } => {
                let in_tx = self.execution.tx_by_connection[*conn].is_some();
                self.with_mut_tx(*conn, *table, |datastore, table_id, tx| {
                    let deleted = datastore.delete_by_rel_mut_tx(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                })?;
                let step = self.step;
                self.with_property_state(|state, access| {
                    properties::on_delete(state, access, step, *conn, *table, row, in_tx)
                })?;
            }
        }

        Ok(())
    }

    fn collect_outcome(&mut self) -> anyhow::Result<DatastoreSimulatorOutcome> {
        let tx = self.datastore.begin_tx(Workload::ForTests);
        let mut final_rows = Vec::with_capacity(self.table_ids.len());
        let mut final_row_counts = Vec::with_capacity(self.table_ids.len());

        for &table_id in &self.table_ids {
            let mut rows = tx
                .table_scan(table_id)?
                .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row.id().unwrap_or_default());
            final_row_counts.push(rows.len() as u64);
            final_rows.push(rows);
        }

        Ok(DatastoreSimulatorOutcome {
            final_row_counts,
            final_rows,
        })
    }

    fn finish(&mut self) {
        for tx in &mut self.execution.tx_by_connection {
            if let Some(tx) = tx.take() {
                let _ = self.datastore.rollback_mut_tx(tx);
            }
        }
        self.execution.active_writer = None;
    }
}

fn bootstrap_datastore() -> spacetimedb_datastore::Result<Locking> {
    Locking::bootstrap(Identity::ZERO, PagePool::new_for_test())
}

fn install_schema(datastore: &Locking, schema: &SchemaPlan) -> anyhow::Result<Vec<TableId>> {
    let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
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

        let table_id = datastore.create_table_mut_tx(
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

    datastore.commit_mut_tx(tx)?;
    Ok(table_ids)
}
