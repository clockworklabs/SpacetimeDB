//! Randomized datastore simulator target built on the shared table workload.

use std::path::Path;

use spacetimedb_datastore::{
    execution_context::Workload,
    locking_tx_datastore::{datastore::Locking, MutTxId},
    traits::{IsolationLevel, MutTx, MutTxDatastore, Tx},
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

pub type DatastoreScenario = TableScenarioId;
pub type DatastoreSimulatorCase = TableWorkloadCase;
pub type Interaction = TableWorkloadInteraction;
pub type DatastoreSimulatorEvent = TableWorkloadEvent;
pub type DatastoreSimulatorOutcome = TableWorkloadOutcome;
pub type DatastoreExecutionFailure = TableWorkloadExecutionFailure;
pub type DatastoreBugArtifact = BugArtifact<DatastoreSimulatorCase, DatastoreExecutionFailure>;
pub type DatastoreRunConfig = RunConfig;
pub use crate::config::parse_duration_spec;

/// DST subsystem wrapper around the randomized datastore simulator.
pub struct DatastoreSimulatorSubsystem;

struct DatastoreTarget;

impl TableTargetHarness for DatastoreTarget {
    type Engine = DatastoreEngine;

    fn target_name() -> &'static str {
        DatastoreSimulatorSubsystem::name()
    }

    fn connection_seed_discriminator() -> u64 {
        17
    }

    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine> {
        DatastoreEngine::new(schema, num_connections)
    }
}

impl DstSubsystem for DatastoreSimulatorSubsystem {
    type Case = DatastoreSimulatorCase;
    type Event = DatastoreSimulatorEvent;
    type Outcome = DatastoreSimulatorOutcome;

    fn name() -> &'static str {
        "datastore-simulator"
    }

    fn generate_case(seed: DstSeed) -> Self::Case {
        harness::generate_case::<DatastoreTarget>(seed, DatastoreScenario::RandomCrud)
    }

    fn run_case(case: &Self::Case) -> anyhow::Result<RunRecord<Self::Case, Self::Event, Self::Outcome>> {
        harness::run_case_detailed::<DatastoreTarget>(case).map_err(|failure| {
            anyhow::anyhow!(
                "datastore simulator failed at step {}: {}",
                failure.step_index,
                failure.reason
            )
        })
    }
}

pub fn generate_case(seed: DstSeed) -> DatastoreSimulatorCase {
    generate_case_for_scenario(seed, DatastoreScenario::RandomCrud)
}

pub fn generate_case_for_scenario(seed: DstSeed, scenario: DatastoreScenario) -> DatastoreSimulatorCase {
    harness::generate_case::<DatastoreTarget>(seed, scenario)
}

pub fn materialize_case(seed: DstSeed, scenario: DatastoreScenario, max_interactions: usize) -> DatastoreSimulatorCase {
    harness::materialize_case::<DatastoreTarget>(seed, scenario, max_interactions)
}

pub fn run_case_detailed(
    case: &DatastoreSimulatorCase,
) -> Result<
    RunRecord<DatastoreSimulatorCase, DatastoreSimulatorEvent, DatastoreSimulatorOutcome>,
    DatastoreExecutionFailure,
> {
    harness::run_case_detailed::<DatastoreTarget>(case)
}

pub fn run_generated_stream(seed: DstSeed, max_interactions: usize) -> anyhow::Result<DatastoreSimulatorOutcome> {
    run_generated_with_config(seed, DatastoreRunConfig::with_max_interactions(max_interactions))
}

pub fn run_generated_with_config(
    seed: DstSeed,
    config: DatastoreRunConfig,
) -> anyhow::Result<DatastoreSimulatorOutcome> {
    run_generated_with_config_and_scenario(seed, DatastoreScenario::RandomCrud, config)
}

pub fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: DatastoreScenario,
    config: DatastoreRunConfig,
) -> anyhow::Result<DatastoreSimulatorOutcome> {
    harness::run_generated_with_config_and_scenario::<DatastoreTarget>(seed, scenario, config)
}

pub fn save_case(path: impl AsRef<Path>, case: &DatastoreSimulatorCase) -> anyhow::Result<()> {
    harness::save_case(path, case)
}

pub fn load_case(path: impl AsRef<Path>) -> anyhow::Result<DatastoreSimulatorCase> {
    harness::load_case(path)
}

pub fn failure_reason(case: &DatastoreSimulatorCase) -> anyhow::Result<String> {
    harness::failure_reason::<DatastoreTarget>(case)
}

pub fn save_bug_artifact(path: impl AsRef<Path>, artifact: &DatastoreBugArtifact) -> anyhow::Result<()> {
    save_json(path, artifact)
}

pub fn load_bug_artifact(path: impl AsRef<Path>) -> anyhow::Result<DatastoreBugArtifact> {
    load_json(path)
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
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
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

#[cfg(test)]
mod tests {
    use std::{
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
    use tempfile::tempdir;

    use crate::{
        runner::{rerun_case, run_generated, verify_repeatable_execution},
        schema::{ColumnPlan, TablePlan},
        seed::DstSeed,
    };

    use super::{
        failure_reason, generate_case, generate_case_for_scenario, load_bug_artifact, parse_duration_spec,
        run_case_detailed, save_bug_artifact, shrink_failure, DatastoreBugArtifact, DatastoreScenario,
        DatastoreSimulatorCase, DatastoreSimulatorSubsystem, Interaction, SchemaPlan, SimRow,
    };
    use crate::workload::table_ops::TableProperty;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn generated_case_replays_identically() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let artifact = run_generated::<DatastoreSimulatorSubsystem>(DstSeed(13)).expect("run datastore simulator case");
        let replayed = rerun_case::<DatastoreSimulatorSubsystem>(&artifact).expect("rerun datastore simulator case");
        assert_eq!(artifact.case, replayed.case);
        assert_eq!(artifact.trace, replayed.trace);
        assert_eq!(artifact.outcome, replayed.outcome);
    }

    #[test]
    fn generated_case_has_repeatable_execution() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let artifact = run_generated::<DatastoreSimulatorSubsystem>(DstSeed(23)).expect("run datastore simulator case");
        let replayed =
            verify_repeatable_execution::<DatastoreSimulatorSubsystem>(&artifact).expect("verify repeatable execution");
        assert_eq!(artifact.trace, replayed.trace);
        assert_eq!(artifact.outcome, replayed.outcome);
    }

    #[test]
    fn failure_reports_stable_reason() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let case = failing_case();
        let failure = run_case_detailed(&case).expect_err("case should fail");
        assert_eq!(failure.step_index, 2);
        assert!(failure.reason.contains("fresh lookup still found deleted row"));
        assert_eq!(failure_reason(&case).expect("extract failure reason"), failure.reason);
    }

    proptest! {
        #[test]
        fn datastore_simulator_holds_across_generated_seeds(seed in any::<u64>()) {
            let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
            run_generated::<DatastoreSimulatorSubsystem>(DstSeed(seed))
                .unwrap_or_else(|err| panic!("seed {seed} failed: {err}"));
        }
    }

    #[test]
    fn duration_specs_parse() {
        assert_eq!(parse_duration_spec("5m").expect("parse 5m"), Duration::from_secs(300));
        assert_eq!(parse_duration_spec("2s").expect("parse 2s"), Duration::from_secs(2));
        assert_eq!(
            parse_duration_spec("10ms").expect("parse 10ms"),
            Duration::from_millis(10)
        );
    }

    #[test]
    fn banking_generation_uses_fixed_schema() {
        let case = generate_case_for_scenario(DstSeed(9090), DatastoreScenario::Banking);
        assert_eq!(case.scenario, DatastoreScenario::Banking);
        assert_eq!(case.schema.tables.len(), 2);
        assert_eq!(case.schema.tables[0].name, "debit_accounts");
        assert_eq!(case.schema.tables[1].name, "credit_accounts");
    }

    #[test]
    fn generated_cases_keep_single_writer_lock() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let case = generate_case(DstSeed(4242));
        let mut owner = None;

        for interaction in case.interactions {
            match interaction {
                Interaction::BeginTx { conn } => {
                    assert_eq!(owner, None, "second writer opened before first closed");
                    owner = Some(conn);
                }
                Interaction::CommitTx { conn } | Interaction::RollbackTx { conn } => {
                    assert_eq!(owner, Some(conn), "non-owner closed writer");
                    owner = None;
                }
                Interaction::Insert { conn, .. } | Interaction::Delete { conn, .. } => {
                    if let Some(writer) = owner {
                        assert_eq!(conn, writer, "interaction ran on non-owner while writer open");
                    }
                }
                Interaction::Check(TableProperty::VisibleInConnection { conn, .. })
                | Interaction::Check(TableProperty::MissingInConnection { conn, .. }) => {
                    if let Some(writer) = owner {
                        assert_eq!(conn, writer, "interaction ran on non-owner while writer open");
                    }
                }
                Interaction::Check(_) => {}
            }
        }

        assert_eq!(owner, None, "writer left open at end of generated case");
    }

    #[test]
    fn second_writer_fails_fast() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let case = DatastoreSimulatorCase {
            seed: DstSeed(88),
            scenario: DatastoreScenario::RandomCrud,
            num_connections: 2,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "locks".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            ty: AlgebraicType::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            ty: AlgebraicType::String,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![Interaction::BeginTx { conn: 0 }, Interaction::BeginTx { conn: 1 }],
        };

        let failure = run_case_detailed(&case).expect_err("second writer should fail");
        assert_eq!(failure.step_index, 1);
        assert!(failure.reason.contains("owns lock"));
    }

    #[test]
    fn bug_artifact_roundtrips() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("bug.json");
        let case = DatastoreSimulatorCase {
            seed: DstSeed(5),
            scenario: DatastoreScenario::RandomCrud,
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            ty: AlgebraicType::U64,
                        },
                        ColumnPlan {
                            name: "ok".into(),
                            ty: AlgebraicType::Bool,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![Interaction::Check(TableProperty::VisibleFresh {
                table: 0,
                row: SimRow {
                    values: vec![AlgebraicValue::U64(7), AlgebraicValue::Bool(true)],
                },
            })],
        };
        let failure = run_case_detailed(&case).expect_err("case should fail");
        let artifact = DatastoreBugArtifact {
            seed: case.seed.0,
            failure,
            case: case.clone(),
            shrunk_case: Some(case),
        };

        save_bug_artifact(&path, &artifact).expect("save artifact");
        let loaded = load_bug_artifact(&path).expect("load artifact");
        assert_eq!(loaded, artifact);
    }

    #[test]
    fn shrink_drops_trailing_noise() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let case = DatastoreSimulatorCase {
            seed: DstSeed(77),
            scenario: DatastoreScenario::RandomCrud,
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            ty: AlgebraicType::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            ty: AlgebraicType::String,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![
                Interaction::Insert {
                    conn: 0,
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(1), AlgebraicValue::String("one".into())],
                    },
                },
                Interaction::Check(TableProperty::VisibleFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(1), AlgebraicValue::String("one".into())],
                    },
                }),
                Interaction::Check(TableProperty::MissingFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(1), AlgebraicValue::String("one".into())],
                    },
                }),
                Interaction::Insert {
                    conn: 0,
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(2), AlgebraicValue::String("two".into())],
                    },
                },
            ],
        };

        let failure = run_case_detailed(&case).expect_err("case should fail");
        let shrunk = shrink_failure(&case, &failure).expect("shrink failure");
        assert!(shrunk.interactions.len() < case.interactions.len());
        let shrunk_failure = run_case_detailed(&shrunk).expect_err("shrunk case should still fail");
        assert_eq!(shrunk_failure.reason, failure.reason);
    }

    fn failing_case() -> DatastoreSimulatorCase {
        DatastoreSimulatorCase {
            seed: DstSeed(99),
            scenario: DatastoreScenario::RandomCrud,
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            ty: AlgebraicType::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            ty: AlgebraicType::String,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![
                Interaction::Insert {
                    conn: 0,
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(1), AlgebraicValue::String("one".into())],
                    },
                },
                Interaction::Check(TableProperty::VisibleFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(1), AlgebraicValue::String("one".into())],
                    },
                }),
                Interaction::Check(TableProperty::MissingFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![AlgebraicValue::U64(1), AlgebraicValue::String("one".into())],
                    },
                }),
            ],
        }
    }
}
