//! Randomized datastore simulator target built on the shared table workload.

use std::path::Path;

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
    targets::harness::{self, TableTargetHarness},
    workload::table_ops::{
        ConnectionWriteState, PropertyBound, TableProperty, TableScenarioId, TableWorkloadCase, TableWorkloadEngine,
        TableWorkloadExecutionFailure, TableWorkloadInteraction, TableWorkloadOutcome,
    },
};

pub type DatastoreSimulatorCase = TableWorkloadCase;
pub type DatastoreSimulatorOutcome = TableWorkloadOutcome;
pub type DatastoreExecutionFailure = TableWorkloadExecutionFailure;
type Interaction = TableWorkloadInteraction;

struct DatastoreTarget;

impl TableTargetHarness for DatastoreTarget {
    type Engine = DatastoreEngine;

    fn connection_seed_discriminator() -> u64 {
        17
    }

    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine> {
        DatastoreEngine::new(schema, num_connections)
    }
}

pub fn materialize_case(seed: DstSeed, scenario: TableScenarioId, max_interactions: usize) -> DatastoreSimulatorCase {
    harness::materialize_case::<DatastoreTarget>(seed, scenario, max_interactions)
}

pub fn run_case_detailed(
    case: &DatastoreSimulatorCase,
) -> Result<DatastoreSimulatorOutcome, DatastoreExecutionFailure> {
    harness::run_case_detailed::<DatastoreTarget>(case)
}

pub fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<DatastoreSimulatorOutcome> {
    harness::run_generated_with_config_and_scenario::<DatastoreTarget>(seed, scenario, config)
}

pub fn save_case(path: impl AsRef<Path>, case: &DatastoreSimulatorCase) -> anyhow::Result<()> {
    harness::save_case(path, case)
}

pub fn load_case(path: impl AsRef<Path>) -> anyhow::Result<DatastoreSimulatorCase> {
    harness::load_case(path)
}

pub fn shrink_failure(
    case: &DatastoreSimulatorCase,
    failure: &DatastoreExecutionFailure,
) -> anyhow::Result<DatastoreSimulatorCase> {
    harness::shrink_failure::<DatastoreTarget>(case, failure)
}

/// Concrete datastore execution harness for the shared table workload.
struct DatastoreEngine {
    datastore: Locking,
    table_ids: Vec<TableId>,
    execution: ConnectionWriteState<MutTxId>,
}

impl DatastoreEngine {
    fn new(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self> {
        let datastore = bootstrap_datastore()?;
        let table_ids = install_schema(&datastore, schema)?;
        Ok(Self {
            datastore,
            table_ids,
            execution: ConnectionWriteState::new(num_connections),
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
        lower: &PropertyBound,
        upper: &PropertyBound,
    ) -> anyhow::Result<Vec<SimRow>> {
        let tx = self.datastore.begin_tx(Workload::ForTests);
        let cols = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let lower = lower.to_range_bound();
        let upper = upper.to_range_bound();
        let rows = self
            .datastore
            .iter_by_col_range_tx(&tx, table_id, cols, (lower, upper))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect();
        Ok(rows)
    }

    fn in_tx_range_scan(
        &self,
        tx: &MutTxId,
        table_id: TableId,
        cols: &[u16],
        lower: &PropertyBound,
        upper: &PropertyBound,
    ) -> anyhow::Result<Vec<SimRow>> {
        let cols = cols.iter().copied().collect::<spacetimedb_primitives::ColList>();
        let lower = lower.to_range_bound();
        let upper = upper.to_range_bound();
        let rows = self
            .datastore
            .iter_by_col_range_mut_tx(tx, table_id, cols, (lower, upper))?
            .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
            .collect();
        Ok(rows)
    }
}

impl TableWorkloadEngine for DatastoreEngine {
    fn execute(&mut self, interaction: &Interaction) -> Result<(), String> {
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
            }
            Interaction::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.datastore.rollback_mut_tx(tx);
                self.execution.active_writer = None;
            }
            Interaction::Insert { conn, table, row } => {
                self.with_mut_tx(*conn, *table, |datastore, table_id, tx| {
                    let bsatn = row.to_bsatn().map_err(|err: anyhow::Error| err.to_string())?;
                    datastore
                        .insert_mut_tx(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(())
                })?;
            }
            Interaction::Delete { conn, table, row } => {
                self.with_mut_tx(*conn, *table, |datastore, table_id, tx| {
                    let deleted = datastore.delete_by_rel_mut_tx(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                })?;
            }
            Interaction::Check(TableProperty::VisibleInConnection { conn, table, row }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
                let found = if let Some(Some(tx)) = self.execution.tx_by_connection.get(*conn) {
                    self.datastore
                        .iter_by_col_eq_mut_tx(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                        .map_err(|err| format!("in-tx lookup failed: {err}"))?
                        .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                        .any(|candidate| candidate == *row)
                } else {
                    self.fresh_lookup(table_id, id)
                        .map_err(|err| format!("fresh lookup failed: {err}"))?
                        == Some(row.clone())
                };
                if !found {
                    return Err(format!("row not visible in connection after write: {row:?}"));
                }
            }
            Interaction::Check(TableProperty::MissingInConnection { conn, table, row }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
                let found = if let Some(Some(tx)) = self.execution.tx_by_connection.get(*conn) {
                    self.datastore
                        .iter_by_col_eq_mut_tx(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                        .map_err(|err| format!("in-tx lookup failed: {err}"))?
                        .next()
                        .is_some()
                } else {
                    self.fresh_lookup(table_id, id)
                        .map_err(|err| format!("fresh lookup failed: {err}"))?
                        .is_some()
                };
                if found {
                    return Err(format!("row still visible in connection after delete: {row:?}"));
                }
            }
            Interaction::Check(TableProperty::VisibleFresh { table, row }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
                let found = self
                    .fresh_lookup(table_id, id)
                    .map_err(|err| format!("fresh lookup failed: {err}"))?;
                if found != Some(row.clone()) {
                    return Err(format!("fresh lookup mismatch: expected={row:?} actual={found:?}"));
                }
            }
            Interaction::Check(TableProperty::MissingFresh { table, row }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
                if self
                    .fresh_lookup(table_id, id)
                    .map_err(|err| format!("fresh lookup failed: {err}"))?
                    .is_some()
                {
                    return Err(format!("fresh lookup still found deleted row: {row:?}"));
                }
            }
            Interaction::Check(TableProperty::RowCountFresh { table, expected }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let actual = self.datastore.begin_tx(Workload::ForTests).row_count(table_id);
                if actual != *expected {
                    return Err(format!("row count mismatch: expected={expected} actual={actual}"));
                }
            }
            Interaction::Check(TableProperty::RangeScanInConnection {
                conn,
                table,
                cols,
                lower,
                upper,
                expected_rows,
            }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let mut actual_rows = if let Some(Some(tx)) = self.execution.tx_by_connection.get(*conn) {
                    self.in_tx_range_scan(tx, table_id, cols, lower, upper)
                        .map_err(|err| format!("in-tx range scan failed: {err}"))?
                } else {
                    self.fresh_range_scan(table_id, cols, lower, upper)
                        .map_err(|err| format!("fresh range scan failed: {err}"))?
                };
                actual_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));
                let mut expected_rows = expected_rows.clone();
                expected_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));
                if actual_rows != expected_rows {
                    return Err(format!(
                        "connection range scan mismatch on table {table}, cols={cols:?}: expected={expected_rows:?} actual={actual_rows:?}"
                    ));
                }
            }
            Interaction::Check(TableProperty::RangeScanFresh {
                table,
                cols,
                lower,
                upper,
                expected_rows,
            }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let mut actual_rows = self
                    .fresh_range_scan(table_id, cols, lower, upper)
                    .map_err(|err| format!("fresh range scan failed: {err}"))?;
                actual_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));
                let mut expected_rows = expected_rows.clone();
                expected_rows.sort_by(|lhs, rhs| compare_rows_by_cols(lhs, rhs, cols));
                if actual_rows != expected_rows {
                    return Err(format!(
                        "fresh range scan mismatch on table {table}, cols={cols:?}: expected={expected_rows:?} actual={actual_rows:?}"
                    ));
                }
            }
            Interaction::Check(TableProperty::TablesMatchFresh { left, right }) => {
                let left_rows = self
                    .collect_rows_for_table(*left)
                    .map_err(|err| format!("left table collect failed: {err}"))?;
                let right_rows = self
                    .collect_rows_for_table(*right)
                    .map_err(|err| format!("right table collect failed: {err}"))?;
                if left_rows != right_rows {
                    return Err(format!(
                        "fresh table mismatch: left_table={left} right_table={right} left={left_rows:?} right={right_rows:?}"
                    ));
                }
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

fn compare_rows_by_cols(lhs: &SimRow, rhs: &SimRow, cols: &[u16]) -> std::cmp::Ordering {
    lhs.project_key(cols)
        .to_algebraic_value()
        .cmp(&rhs.project_key(cols).to_algebraic_value())
        .then_with(|| lhs.values.cmp(&rhs.values))
}
