//! Basic RelationalDB simulator target using the shared table workload.

use std::path::Path;

use spacetimedb_core::{
    db::relational_db::{MutTx as RelMutTx, RelationalDB, Tx as RelTx},
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
    bugbase::{load_json, save_json, BugArtifact},
    config::RunConfig,
    schema::{SchemaPlan, SimRow},
    seed::DstSeed,
    subsystem::{DstSubsystem, RunRecord},
    targets::harness::{self, TableTargetHarness},
    workload::table_ops::{
        ConnectionWriteState, TableProperty, TableScenarioId, TableWorkloadCase, TableWorkloadEngine,
        TableWorkloadEvent, TableWorkloadExecutionFailure, TableWorkloadInteraction, TableWorkloadOutcome,
    },
};

pub type RelationalDbScenario = TableScenarioId;
pub type RelationalDbSimulatorCase = TableWorkloadCase;
pub type RelationalDbInteraction = TableWorkloadInteraction;
pub type RelationalDbSimulatorEvent = TableWorkloadEvent;
pub type RelationalDbSimulatorOutcome = TableWorkloadOutcome;
pub type RelationalDbExecutionFailure = TableWorkloadExecutionFailure;
pub type RelationalDbBugArtifact = BugArtifact<RelationalDbSimulatorCase, RelationalDbExecutionFailure>;
pub type RelationalDbRunConfig = RunConfig;

/// DST subsystem wrapper around the relational-db simulator target.
pub struct RelationalDbSimulatorSubsystem;

struct RelationalDbTarget;

impl TableTargetHarness for RelationalDbTarget {
    type Engine = RelationalDbEngine;

    fn target_name() -> &'static str {
        RelationalDbSimulatorSubsystem::name()
    }

    fn connection_seed_discriminator() -> u64 {
        31
    }

    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine> {
        RelationalDbEngine::new(schema, num_connections)
    }
}

impl DstSubsystem for RelationalDbSimulatorSubsystem {
    type Case = RelationalDbSimulatorCase;
    type Event = RelationalDbSimulatorEvent;
    type Outcome = RelationalDbSimulatorOutcome;

    fn name() -> &'static str {
        "relational-db-simulator"
    }

    fn generate_case(seed: DstSeed) -> Self::Case {
        harness::generate_case::<RelationalDbTarget>(seed, RelationalDbScenario::RandomCrud)
    }

    fn run_case(case: &Self::Case) -> anyhow::Result<RunRecord<Self::Case, Self::Event, Self::Outcome>> {
        harness::run_case_detailed::<RelationalDbTarget>(case).map_err(|failure| {
            anyhow::anyhow!(
                "relational db simulator failed at step {}: {}",
                failure.step_index,
                failure.reason
            )
        })
    }
}

pub fn generate_case(seed: DstSeed) -> RelationalDbSimulatorCase {
    generate_case_for_scenario(seed, RelationalDbScenario::RandomCrud)
}

pub fn generate_case_for_scenario(seed: DstSeed, scenario: RelationalDbScenario) -> RelationalDbSimulatorCase {
    harness::generate_case::<RelationalDbTarget>(seed, scenario)
}

pub fn materialize_case(
    seed: DstSeed,
    scenario: RelationalDbScenario,
    max_interactions: usize,
) -> RelationalDbSimulatorCase {
    harness::materialize_case::<RelationalDbTarget>(seed, scenario, max_interactions)
}

pub fn run_case_detailed(
    case: &RelationalDbSimulatorCase,
) -> Result<
    RunRecord<RelationalDbSimulatorCase, RelationalDbSimulatorEvent, RelationalDbSimulatorOutcome>,
    RelationalDbExecutionFailure,
> {
    harness::run_case_detailed::<RelationalDbTarget>(case)
}

pub fn run_generated_stream(seed: DstSeed, max_interactions: usize) -> anyhow::Result<RelationalDbSimulatorOutcome> {
    run_generated_with_config(seed, RelationalDbRunConfig::with_max_interactions(max_interactions))
}

pub fn run_generated_with_config(
    seed: DstSeed,
    config: RelationalDbRunConfig,
) -> anyhow::Result<RelationalDbSimulatorOutcome> {
    run_generated_with_config_and_scenario(seed, RelationalDbScenario::RandomCrud, config)
}

pub fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: RelationalDbScenario,
    config: RelationalDbRunConfig,
) -> anyhow::Result<RelationalDbSimulatorOutcome> {
    harness::run_generated_with_config_and_scenario::<RelationalDbTarget>(seed, scenario, config)
}

pub fn save_case(path: impl AsRef<Path>, case: &RelationalDbSimulatorCase) -> anyhow::Result<()> {
    harness::save_case(path, case)
}

pub fn load_case(path: impl AsRef<Path>) -> anyhow::Result<RelationalDbSimulatorCase> {
    harness::load_case(path)
}

pub fn save_bug_artifact(path: impl AsRef<Path>, artifact: &RelationalDbBugArtifact) -> anyhow::Result<()> {
    save_json(path, artifact)
}

pub fn load_bug_artifact(path: impl AsRef<Path>) -> anyhow::Result<RelationalDbBugArtifact> {
    load_json(path)
}

pub fn shrink_failure(
    case: &RelationalDbSimulatorCase,
    failure: &RelationalDbExecutionFailure,
) -> anyhow::Result<RelationalDbSimulatorCase> {
    harness::shrink_failure::<RelationalDbTarget>(case, failure)
}

/// Concrete `RelationalDB` execution harness for the shared table workload.
struct RelationalDbEngine {
    db: RelationalDB,
    table_ids: Vec<TableId>,
    execution: ConnectionWriteState<RelMutTx>,
}

impl RelationalDbEngine {
    fn new(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self> {
        let db = bootstrap_relational_db()?;
        let table_ids = install_schema(&db, schema)?;
        Ok(Self {
            db,
            table_ids,
            execution: ConnectionWriteState::new(num_connections),
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
}

impl TableWorkloadEngine for RelationalDbEngine {
    fn execute(&mut self, interaction: &RelationalDbInteraction) -> Result<(), String> {
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
            }
            RelationalDbInteraction::RollbackTx { conn } => {
                self.execution.ensure_writer_owner(*conn, "rollback")?;
                let tx = self.execution.tx_by_connection[*conn]
                    .take()
                    .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
                let _ = self.db.rollback_mut_tx(tx);
                self.execution.active_writer = None;
            }
            RelationalDbInteraction::Insert { conn, table, row } => {
                self.with_mut_tx(*conn, *table, |db, table_id, tx| {
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    db.insert(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(())
                })?;
            }
            RelationalDbInteraction::Delete { conn, table, row } => {
                self.with_mut_tx(*conn, *table, |db, table_id, tx| {
                    let deleted = db.delete_by_rel(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                })?;
            }
            RelationalDbInteraction::Check(TableProperty::VisibleInConnection { conn, table, row }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
                let found = if let Some(Some(tx)) = self.execution.tx_by_connection.get(*conn) {
                    self.db
                        .iter_by_col_eq_mut(tx, table_id, 0u16, &AlgebraicValue::U64(id))
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
            RelationalDbInteraction::Check(TableProperty::MissingInConnection { conn, table, row }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
                let found = if let Some(Some(tx)) = self.execution.tx_by_connection.get(*conn) {
                    self.db
                        .iter_by_col_eq_mut(tx, table_id, 0u16, &AlgebraicValue::U64(id))
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
            RelationalDbInteraction::Check(TableProperty::VisibleFresh { table, row }) => {
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
            RelationalDbInteraction::Check(TableProperty::MissingFresh { table, row }) => {
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
            RelationalDbInteraction::Check(TableProperty::RowCountFresh { table, expected }) => {
                let table_id = *self
                    .table_ids
                    .get(*table)
                    .ok_or_else(|| format!("table {table} out of range"))?;
                let tx: RelTx = self.db.begin_tx(Workload::ForTests);
                let actual = self
                    .db
                    .iter(&tx, table_id)
                    .map_err(|err| format!("row count scan failed: {err}"))?
                    .count() as u64;
                let _ = self.db.release_tx(tx);
                if actual != *expected {
                    return Err(format!("row count mismatch: expected={expected} actual={actual}"));
                }
            }
            RelationalDbInteraction::Check(TableProperty::TablesMatchFresh { left, right }) => {
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
        if let Some(col) = table.secondary_index_col {
            indexes.push(IndexSchema::for_test(
                format!("{}_c{col}_idx", table.name),
                BTreeAlgorithm::from(col),
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

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use pretty_assertions::assert_eq;

    use crate::{
        runner::{rerun_case, run_generated},
        seed::DstSeed,
    };

    use super::{generate_case_for_scenario, RelationalDbScenario, RelationalDbSimulatorSubsystem};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn generated_case_replays_identically() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let artifact = run_generated::<RelationalDbSimulatorSubsystem>(DstSeed(13)).expect("run relational db case");
        let replayed = rerun_case::<RelationalDbSimulatorSubsystem>(&artifact).expect("rerun relational db case");
        assert_eq!(artifact.case, replayed.case);
        assert_eq!(artifact.trace, replayed.trace);
        assert_eq!(artifact.outcome, replayed.outcome);
    }

    #[test]
    fn banking_generation_uses_fixed_schema() {
        let case = generate_case_for_scenario(DstSeed(4242), RelationalDbScenario::Banking);
        assert_eq!(case.scenario, RelationalDbScenario::Banking);
        assert_eq!(case.schema.tables.len(), 2);
        assert_eq!(case.schema.tables[0].name, "debit_accounts");
        assert_eq!(case.schema.tables[1].name, "credit_accounts");
    }
}
