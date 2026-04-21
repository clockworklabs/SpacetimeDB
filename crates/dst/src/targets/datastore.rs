//! Randomized datastore simulator target.
//!
//! This is the highest-level subsystem in the crate:
//!
//! - generate a schema,
//! - generate a deterministic interaction stream or plan,
//! - execute the plan against a real datastore instance,
//! - compare the final committed datastore state against an in-memory model.
//!
//! The file is large, so it is easiest to read in this order:
//!
//! 1. case and interaction types,
//! 2. `generate_case` and `InteractionStream`,
//! 3. `run_case_detailed` / `run_generated_stream`,
//! 4. `execute_interaction`,
//! 5. `GenerationModel`,
//! 6. `ExpectedModel`.

use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};
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
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;

use crate::{
    bugbase::{load_json, save_json, BugArtifact},
    seed::{DstRng, DstSeed},
    shrink::shrink_by_removing,
    subsystem::{DstSubsystem, RunRecord},
    trace::Trace,
};

/// Full input for one randomized datastore simulator run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreSimulatorCase {
    pub seed: DstSeed,
    pub num_connections: usize,
    pub schema: SchemaPlan,
    pub interactions: Vec<Interaction>,
}

/// Generated schema for one simulator case.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchemaPlan {
    pub tables: Vec<TablePlan>,
}

/// Table definition used by the simulator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TablePlan {
    pub name: String,
    pub columns: Vec<ColumnPlan>,
    pub secondary_index_col: Option<u16>,
}

/// Column definition used by the simulator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ColumnPlan {
    pub name: String,
    pub kind: ColumnKind,
}

/// Small set of column kinds currently supported by the simulator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ColumnKind {
    U64,
    String,
    Bool,
}

/// Serializable row representation used by generated interactions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimRow {
    pub values: Vec<SimValue>,
}

/// Serializable cell value used by generated interactions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SimValue {
    U64(u64),
    String(String),
    Bool(bool),
}

/// One generated simulator step.
///
/// The plan intentionally mixes mutations with immediate assertions so failures
/// are attributed to the first step that violates an invariant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Interaction {
    BeginTx { conn: usize },
    CommitTx { conn: usize },
    RollbackTx { conn: usize },
    Insert { conn: usize, table: usize, row: SimRow },
    Delete { conn: usize, table: usize, row: SimRow },
    AssertVisibleInConnection { conn: usize, table: usize, row: SimRow },
    AssertMissingInConnection { conn: usize, table: usize, row: SimRow },
    AssertVisibleFresh { table: usize, row: SimRow },
    AssertMissingFresh { table: usize, row: SimRow },
    AssertRowCountFresh { table: usize, expected: u64 },
}

/// Trace event for the datastore simulator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DatastoreSimulatorEvent {
    Executed(Interaction),
}

/// Final state collected from the datastore after the run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreSimulatorOutcome {
    pub final_row_counts: Vec<u64>,
    pub final_rows: Vec<Vec<SimRow>>,
}

/// Rich failure returned by `run_case_detailed`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreExecutionFailure {
    pub step_index: usize,
    pub reason: String,
    pub interaction: Interaction,
}

pub type DatastoreBugArtifact = BugArtifact<DatastoreSimulatorCase, DatastoreExecutionFailure>;

/// DST subsystem wrapper around the randomized datastore simulator.
pub struct DatastoreSimulatorSubsystem;

impl DstSubsystem for DatastoreSimulatorSubsystem {
    type Case = DatastoreSimulatorCase;
    type Event = DatastoreSimulatorEvent;
    type Outcome = DatastoreSimulatorOutcome;

    fn name() -> &'static str {
        "datastore-simulator"
    }

    fn generate_case(seed: DstSeed) -> Self::Case {
        generate_case(seed)
    }

    fn run_case(case: &Self::Case) -> anyhow::Result<RunRecord<Self::Case, Self::Event, Self::Outcome>> {
        run_case_detailed(case).map_err(|failure| {
            anyhow::anyhow!(
                "datastore simulator failed at step {}: {}",
                failure.step_index,
                failure.reason
            )
        })
    }
}

/// Generates a deterministic simulator case from a seed.
pub fn generate_case(seed: DstSeed) -> DatastoreSimulatorCase {
    let mut rng = seed.fork(17).rng();
    let num_connections = rng.index(3) + 1;
    let schema = generate_schema(&mut rng);
    let interactions =
        InteractionStream::new(seed, schema.clone(), num_connections, default_target_ops(&mut rng)).collect();
    DatastoreSimulatorCase {
        seed,
        num_connections,
        schema,
        interactions,
    }
}

/// Executes a generated case and returns either a full run record or the first
/// failing interaction.
pub fn run_case_detailed(
    case: &DatastoreSimulatorCase,
) -> Result<
    RunRecord<DatastoreSimulatorCase, DatastoreSimulatorEvent, DatastoreSimulatorOutcome>,
    DatastoreExecutionFailure,
> {
    run_interactions(
        case.seed,
        case.schema.clone(),
        case.num_connections,
        case.interactions.iter().cloned(),
        Some(case.clone()),
    )
}

/// Executes a generated simulator workload without first materializing all
/// interactions in memory.
pub fn run_generated_stream(seed: DstSeed, max_interactions: usize) -> anyhow::Result<DatastoreSimulatorOutcome> {
    let mut rng = seed.fork(17).rng();
    let num_connections = rng.index(3) + 1;
    let schema = generate_schema(&mut rng);
    let stream = InteractionStream::new(seed, schema.clone(), num_connections, max_interactions);
    let datastore = bootstrap_datastore()?;
    let table_ids = install_schema(&datastore, &schema)?;
    let mut execution = ExecutionState::new(num_connections);
    let mut expected = ExpectedModel::new(table_ids.len(), num_connections);

    for (step_index, interaction) in stream.enumerate() {
        execute_interaction(&datastore, &table_ids, &mut execution, &interaction).map_err(|reason| {
            anyhow::anyhow!("datastore simulator failed at step {step_index}: {reason}")
        })?;
        expected.apply(&interaction);
    }

    execution.rollback_all(&datastore);

    let outcome = collect_outcome(&datastore, &table_ids)?;
    let expected_rows = expected.committed_rows();
    if outcome.final_rows != expected_rows {
        anyhow::bail!(
            "final datastore state mismatch: expected={expected_rows:?} actual={:?}",
            outcome.final_rows
        );
    }

    Ok(outcome)
}

fn run_interactions(
    seed: DstSeed,
    schema: SchemaPlan,
    num_connections: usize,
    interactions: impl IntoIterator<Item = Interaction>,
    case_override: Option<DatastoreSimulatorCase>,
) -> Result<
    RunRecord<DatastoreSimulatorCase, DatastoreSimulatorEvent, DatastoreSimulatorOutcome>,
    DatastoreExecutionFailure,
> {
    let datastore = bootstrap_datastore().map_err(|err| failure_without_step(format!("bootstrap failed: {err}")))?;
    let table_ids = install_schema(&datastore, &schema)
        .map_err(|err| failure_without_step(format!("schema install failed: {err}")))?;
    let mut trace = Trace::default();
    let mut execution = ExecutionState::new(num_connections);
    let mut expected = ExpectedModel::new(table_ids.len(), num_connections);
    let mut executed_interactions = Vec::new();

    for (step_index, interaction) in interactions.into_iter().enumerate() {
        trace.push(DatastoreSimulatorEvent::Executed(interaction.clone()));
        execute_interaction(&datastore, &table_ids, &mut execution, &interaction).map_err(|reason| {
            DatastoreExecutionFailure {
                step_index,
                reason,
                interaction: interaction.clone(),
            }
        })?;
        expected.apply(&interaction);
        executed_interactions.push(interaction);
    }

    execution.rollback_all(&datastore);

    let outcome = collect_outcome(&datastore, &table_ids)
        .map_err(|err| failure_without_step(format!("collect outcome failed: {err}")))?;
    let expected_rows = expected.committed_rows();
    if outcome.final_rows != expected_rows {
        return Err(failure_without_step(format!(
            "final datastore state mismatch: expected={expected_rows:?} actual={:?}",
            outcome.final_rows
        )));
    }

    let case = case_override.unwrap_or(DatastoreSimulatorCase {
        seed,
        num_connections,
        schema,
        interactions: executed_interactions,
    });

    Ok(RunRecord {
        subsystem: DatastoreSimulatorSubsystem::name(),
        seed,
        case,
        trace: Some(trace),
        outcome,
    })
}

/// Saves a simulator case as JSON for replay or debugging.
pub fn save_case(path: impl AsRef<Path>, case: &DatastoreSimulatorCase) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(case)?;
    fs::write(path, body)?;
    Ok(())
}

/// Loads a simulator case previously written by [`save_case`].
pub fn load_case(path: impl AsRef<Path>) -> anyhow::Result<DatastoreSimulatorCase> {
    let body = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}

/// Runs a case and extracts only the failure reason.
pub fn failure_reason(case: &DatastoreSimulatorCase) -> anyhow::Result<String> {
    match run_case_detailed(case) {
        Ok(_) => anyhow::bail!("case did not fail"),
        Err(failure) => Ok(failure.reason),
    }
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
    shrink_by_removing(
        case,
        failure,
        |case| {
            let mut shrunk = case.clone();
            shrunk.interactions.truncate(failure.step_index.saturating_add(1));
            shrunk
        },
        |case| case.interactions.len(),
        remove_interaction,
        |case| match run_case_detailed(case) {
            Ok(_) => anyhow::bail!("case did not fail"),
            Err(failure) => Ok(failure),
        },
        |expected, candidate| expected.reason == candidate.reason,
    )
}

fn remove_interaction(case: &DatastoreSimulatorCase, idx: usize) -> Option<DatastoreSimulatorCase> {
    let interaction = case.interactions.get(idx)?;
    if matches!(
        interaction,
        Interaction::CommitTx { .. } | Interaction::RollbackTx { .. }
    ) {
        return None;
    }

    let mut interactions = case.interactions.clone();
    interactions.remove(idx);
    Some(DatastoreSimulatorCase {
        seed: case.seed,
        num_connections: case.num_connections,
        schema: case.schema.clone(),
        interactions,
    })
}

fn generate_schema(rng: &mut DstRng) -> SchemaPlan {
    let table_count = rng.index(3) + 1;
    let mut tables = Vec::with_capacity(table_count);

    for table_idx in 0..table_count {
        let extra_cols = rng.index(3);
        let mut columns = vec![ColumnPlan {
            name: "id".into(),
            kind: ColumnKind::U64,
        }];
        for col_idx in 0..extra_cols {
            columns.push(ColumnPlan {
                name: format!("c{table_idx}_{col_idx}"),
                kind: match rng.index(3) {
                    0 => ColumnKind::U64,
                    1 => ColumnKind::String,
                    _ => ColumnKind::Bool,
                },
            });
        }
        let secondary_index_col = (columns.len() > 1 && rng.index(100) < 50).then_some(1);
        tables.push(TablePlan {
            name: format!("dst_table_{table_idx}_{}", rng.next_u64() % 10_000),
            columns,
            secondary_index_col,
        });
    }

    SchemaPlan { tables }
}

fn default_target_ops(rng: &mut DstRng) -> usize {
    24 + rng.index(24)
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
            .map(|(idx, col)| ColumnSchema::for_test(idx as u16, &col.name, col.kind.to_algebraic_type()))
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

fn execute_interaction(
    datastore: &Locking,
    table_ids: &[TableId],
    execution: &mut ExecutionState,
    interaction: &Interaction,
) -> Result<(), String> {
    match interaction {
        Interaction::BeginTx { conn } => {
            execution.ensure_known_connection(*conn)?;
            if execution.tx_by_connection[*conn].is_some() {
                return Err(format!("connection {conn} already has open transaction"));
            }
            if let Some(owner) = execution.active_writer {
                return Err(format!(
                    "connection {conn} cannot begin write transaction while connection {owner} owns lock"
                ));
            }
            execution.tx_by_connection[*conn] =
                Some(datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests));
            execution.active_writer = Some(*conn);
        }
        Interaction::CommitTx { conn } => {
            execution.ensure_writer_owner(*conn, "commit")?;
            let tx = execution.tx_by_connection[*conn]
                .take()
                .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
            datastore
                .commit_mut_tx(tx)
                .map_err(|err| format!("commit failed on connection {conn}: {err}"))?;
            execution.active_writer = None;
        }
        Interaction::RollbackTx { conn } => {
            execution.ensure_writer_owner(*conn, "rollback")?;
            let tx = execution.tx_by_connection[*conn]
                .take()
                .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
            let _ = datastore.rollback_mut_tx(tx);
            execution.active_writer = None;
        }
        Interaction::Insert { conn, table, row } => {
            with_mut_tx(
                datastore,
                table_ids,
                execution,
                *conn,
                *table,
                |datastore, table_id, tx| {
                    let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
                    datastore
                        .insert_mut_tx(tx, table_id, &bsatn)
                        .map_err(|err| format!("insert failed: {err}"))?;
                    Ok(())
                },
            )?;
        }
        Interaction::Delete { conn, table, row } => {
            with_mut_tx(
                datastore,
                table_ids,
                execution,
                *conn,
                *table,
                |datastore, table_id, tx| {
                    let deleted = datastore.delete_by_rel_mut_tx(tx, table_id, [row.to_product_value()]);
                    if deleted != 1 {
                        return Err(format!("delete expected 1 row, got {deleted}"));
                    }
                    Ok(())
                },
            )?;
        }
        Interaction::AssertVisibleInConnection { conn, table, row } => {
            let table_id = *table_ids
                .get(*table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
            let found = if let Some(Some(tx)) = execution.tx_by_connection.get(*conn) {
                datastore
                    .iter_by_col_eq_mut_tx(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                    .map_err(|err| format!("in-tx lookup failed: {err}"))?
                    .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
                    .any(|candidate| candidate == *row)
            } else {
                fresh_lookup(datastore, table_id, id).map_err(|err| format!("fresh lookup failed: {err}"))?
                    == Some(row.clone())
            };
            if !found {
                return Err(format!("row not visible in connection after write: {row:?}"));
            }
        }
        Interaction::AssertMissingInConnection { conn, table, row } => {
            let table_id = *table_ids
                .get(*table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
            let found = if let Some(Some(tx)) = execution.tx_by_connection.get(*conn) {
                datastore
                    .iter_by_col_eq_mut_tx(tx, table_id, 0u16, &AlgebraicValue::U64(id))
                    .map_err(|err| format!("in-tx lookup failed: {err}"))?
                    .next()
                    .is_some()
            } else {
                fresh_lookup(datastore, table_id, id)
                    .map_err(|err| format!("fresh lookup failed: {err}"))?
                    .is_some()
            };
            if found {
                return Err(format!("row still visible in connection after delete: {row:?}"));
            }
        }
        Interaction::AssertVisibleFresh { table, row } => {
            let table_id = *table_ids
                .get(*table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
            let found = fresh_lookup(datastore, table_id, id).map_err(|err| format!("fresh lookup failed: {err}"))?;
            if found != Some(row.clone()) {
                return Err(format!("fresh lookup mismatch: expected={row:?} actual={found:?}"));
            }
        }
        Interaction::AssertMissingFresh { table, row } => {
            let table_id = *table_ids
                .get(*table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let id = row.id().ok_or_else(|| "row missing id column".to_string())?;
            if fresh_lookup(datastore, table_id, id)
                .map_err(|err| format!("fresh lookup failed: {err}"))?
                .is_some()
            {
                return Err(format!("fresh lookup still found deleted row: {row:?}"));
            }
        }
        Interaction::AssertRowCountFresh { table, expected } => {
            let table_id = *table_ids
                .get(*table)
                .ok_or_else(|| format!("table {table} out of range"))?;
            let actual = datastore.begin_tx(Workload::ForTests).row_count(table_id);
            if actual != *expected {
                return Err(format!("row count mismatch: expected={expected} actual={actual}"));
            }
        }
    }

    Ok(())
}

fn with_mut_tx(
    datastore: &Locking,
    table_ids: &[TableId],
    execution: &mut ExecutionState,
    conn: usize,
    table: usize,
    mut f: impl FnMut(&Locking, TableId, &mut MutTxId) -> Result<(), String>,
) -> Result<(), String> {
    let table_id = *table_ids
        .get(table)
        .ok_or_else(|| format!("table {table} out of range"))?;
    execution.ensure_known_connection(conn)?;
    let slot = &mut execution.tx_by_connection[conn];

    match slot {
        Some(tx) => f(datastore, table_id, tx),
        None => {
            if let Some(owner) = execution.active_writer {
                return Err(format!(
                    "connection {conn} cannot auto-commit write while connection {owner} owns lock"
                ));
            }
            let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
            execution.active_writer = Some(conn);
            f(datastore, table_id, &mut tx)?;
            datastore
                .commit_mut_tx(tx)
                .map_err(|err| format!("auto-commit failed on connection {conn}: {err}"))?;
            execution.active_writer = None;
            Ok(())
        }
    }
}

fn fresh_lookup(datastore: &Locking, table_id: TableId, id: u64) -> anyhow::Result<Option<SimRow>> {
    let tx = datastore.begin_tx(Workload::ForTests);
    Ok(tx
        .table_scan(table_id)?
        .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
        .find(|row| row.id() == Some(id)))
}

fn collect_outcome(datastore: &Locking, table_ids: &[TableId]) -> anyhow::Result<DatastoreSimulatorOutcome> {
    let tx = datastore.begin_tx(Workload::ForTests);
    let mut final_rows = Vec::with_capacity(table_ids.len());
    let mut final_row_counts = Vec::with_capacity(table_ids.len());

    for &table_id in table_ids {
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

fn failure_without_step(reason: String) -> DatastoreExecutionFailure {
    DatastoreExecutionFailure {
        step_index: usize::MAX,
        reason,
        interaction: Interaction::AssertRowCountFresh {
            table: usize::MAX,
            expected: 0,
        },
    }
}

impl ColumnKind {
    fn to_algebraic_type(&self) -> AlgebraicType {
        match self {
            ColumnKind::U64 => AlgebraicType::U64,
            ColumnKind::String => AlgebraicType::String,
            ColumnKind::Bool => AlgebraicType::Bool,
        }
    }
}

impl SimValue {
    fn to_algebraic_value(&self) -> AlgebraicValue {
        match self {
            SimValue::U64(value) => AlgebraicValue::U64(*value),
            SimValue::String(value) => AlgebraicValue::String(value.clone().into()),
            SimValue::Bool(value) => AlgebraicValue::Bool(*value),
        }
    }

    fn from_algebraic_value(value: AlgebraicValue) -> Self {
        match value {
            AlgebraicValue::U64(value) => SimValue::U64(value),
            AlgebraicValue::String(value) => SimValue::String(value.to_string()),
            AlgebraicValue::Bool(value) => SimValue::Bool(value),
            other => panic!("unsupported value in simulator row: {other:?}"),
        }
    }
}

impl SimRow {
    fn to_product_value(&self) -> ProductValue {
        ProductValue::from_iter(self.values.iter().map(SimValue::to_algebraic_value))
    }

    fn to_bsatn(&self) -> anyhow::Result<Vec<u8>> {
        Ok(spacetimedb_sats::bsatn::to_vec(&self.to_product_value())?)
    }

    fn from_product_value(value: ProductValue) -> Self {
        SimRow {
            values: value.elements.into_iter().map(SimValue::from_algebraic_value).collect(),
        }
    }

    fn id(&self) -> Option<u64> {
        match self.values.first() {
            Some(SimValue::U64(value)) => Some(*value),
            _ => None,
        }
    }
}

struct ExecutionState {
    tx_by_connection: Vec<Option<MutTxId>>,
    active_writer: Option<usize>,
}

impl ExecutionState {
    fn new(connection_count: usize) -> Self {
        Self {
            tx_by_connection: (0..connection_count).map(|_| None).collect(),
            active_writer: None,
        }
    }

    fn ensure_known_connection(&self, conn: usize) -> Result<(), String> {
        self.tx_by_connection
            .get(conn)
            .map(|_| ())
            .ok_or_else(|| format!("connection {conn} out of range"))
    }

    fn ensure_writer_owner(&self, conn: usize, action: &str) -> Result<(), String> {
        self.ensure_known_connection(conn)?;
        match self.active_writer {
            Some(owner) if owner == conn => Ok(()),
            Some(owner) => Err(format!(
                "connection {conn} cannot {action} while connection {owner} owns lock"
            )),
            None => Err(format!("connection {conn} has no transaction to {action}")),
        }
    }

    fn rollback_all(&mut self, datastore: &Locking) {
        for tx in &mut self.tx_by_connection {
            if let Some(tx) = tx.take() {
                let _ = datastore.rollback_mut_tx(tx);
            }
        }
        self.active_writer = None;
    }
}

#[derive(Clone, Debug)]
struct InteractionStream {
    rng: DstRng,
    model: GenerationModel,
    num_connections: usize,
    target_interactions: usize,
    emitted: usize,
    finalize_conn: usize,
    pending: VecDeque<Interaction>,
    finished: bool,
}

impl InteractionStream {
    fn new(seed: DstSeed, schema: SchemaPlan, num_connections: usize, target_interactions: usize) -> Self {
        Self {
            rng: seed.fork(17).rng(),
            model: GenerationModel::new(&schema, num_connections, seed),
            num_connections,
            target_interactions,
            emitted: 0,
            finalize_conn: 0,
            pending: VecDeque::new(),
            finished: false,
        }
    }

    fn fill_pending(&mut self) {
        if self.emitted >= self.target_interactions {
            while self.finalize_conn < self.num_connections {
                let conn = self.finalize_conn;
                self.finalize_conn += 1;
                if self.model.connections[conn].in_tx {
                    let followups = self.model.commit(conn);
                    self.pending.push_back(Interaction::CommitTx { conn });
                    self.pending.extend(followups);
                    return;
                }
            }
            self.finished = true;
            return;
        }

        let conn = self
            .model
            .active_writer()
            .unwrap_or_else(|| self.rng.index(self.num_connections));

        if !self.model.connections[conn].in_tx && self.model.active_writer().is_none() && self.rng.index(100) < 20 {
            self.model.begin_tx(conn);
            self.pending.push_back(Interaction::BeginTx { conn });
            return;
        }

        if self.model.connections[conn].in_tx && self.rng.index(100) < 15 {
            let followups = self.model.commit(conn);
            self.pending.push_back(Interaction::CommitTx { conn });
            self.pending.extend(followups);
            return;
        }

        if self.model.connections[conn].in_tx && self.rng.index(100) < 10 {
            let followups = self.model.rollback(conn);
            self.pending.push_back(Interaction::RollbackTx { conn });
            self.pending.extend(followups);
            return;
        }

        let table = self.rng.index(self.model.schema.tables.len());
        let visible_rows = self.model.visible_rows(conn, table);
        let choose_insert = visible_rows.is_empty() || self.rng.index(100) < 65;
        if choose_insert {
            let row = self.model.make_row(&mut self.rng, table);
            self.model.insert(conn, table, row.clone());
            self.pending.push_back(Interaction::Insert {
                conn,
                table,
                row: row.clone(),
            });
            self.pending.push_back(Interaction::AssertVisibleInConnection { conn, table, row });
            if !self.model.connections[conn].in_tx {
                let row = self.model.last_inserted_row(conn).expect("tracked auto-commit insert");
                self.pending.push_back(Interaction::AssertVisibleFresh { table, row });
            }
            return;
        }

        let row = visible_rows[self.rng.index(visible_rows.len())].clone();
        self.model.delete(conn, table, row.clone());
        self.pending.push_back(Interaction::Delete {
            conn,
            table,
            row: row.clone(),
        });
        self.pending.push_back(Interaction::AssertMissingInConnection {
            conn,
            table,
            row: row.clone(),
        });
        if !self.model.connections[conn].in_tx {
            self.pending.push_back(Interaction::AssertMissingFresh { table, row });
        }
    }
}

impl Iterator for InteractionStream {
    type Item = Interaction;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(interaction) = self.pending.pop_front() {
                self.emitted += 1;
                return Some(interaction);
            }

            if self.finished {
                return None;
            }

            self.fill_pending();
        }
    }
}

#[derive(Clone, Debug)]
struct GenerationModel {
    schema: SchemaPlan,
    connections: Vec<PendingConnection>,
    committed: Vec<Vec<SimRow>>,
    next_ids: Vec<u64>,
    active_writer: Option<usize>,
}

#[derive(Clone, Debug, Default)]
struct PendingConnection {
    in_tx: bool,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
    last_auto_committed_insert: Option<SimRow>,
}

impl GenerationModel {
    fn new(schema: &SchemaPlan, num_connections: usize, seed: DstSeed) -> Self {
        Self {
            schema: schema.clone(),
            connections: vec![PendingConnection::default(); num_connections],
            committed: vec![Vec::new(); schema.tables.len()],
            next_ids: (0..schema.tables.len())
                .map(|idx| seed.fork(idx as u64 + 100).0)
                .collect(),
            active_writer: None,
        }
    }

    fn make_row(&mut self, rng: &mut DstRng, table: usize) -> SimRow {
        let table_plan = &self.schema.tables[table];
        let id = self.next_ids[table];
        self.next_ids[table] = self.next_ids[table].wrapping_add(1).max(1);
        let mut values = vec![SimValue::U64(id)];
        for (idx, col) in table_plan.columns.iter().enumerate().skip(1) {
            values.push(match col.kind {
                ColumnKind::U64 => SimValue::U64((rng.next_u64() % 1000) + idx as u64),
                ColumnKind::String => SimValue::String(format!("v{}_{}", idx, rng.next_u64() % 10_000)),
                ColumnKind::Bool => SimValue::Bool(rng.index(2) == 0),
            });
        }
        SimRow { values }
    }

    fn visible_rows(&self, conn: usize, table: usize) -> Vec<SimRow> {
        let mut rows = self.committed[table].clone();
        let pending = &self.connections[conn];
        for (pending_table, row) in &pending.staged_deletes {
            if *pending_table == table {
                rows.retain(|candidate| candidate != row);
            }
        }
        for (pending_table, row) in &pending.staged_inserts {
            if *pending_table == table {
                rows.push(row.clone());
            }
        }
        rows
    }

    fn active_writer(&self) -> Option<usize> {
        self.active_writer
    }

    fn begin_tx(&mut self, conn: usize) {
        assert!(self.active_writer.is_none(), "single writer already active");
        let pending = &mut self.connections[conn];
        assert!(!pending.in_tx, "connection already in transaction");
        pending.in_tx = true;
        self.active_writer = Some(conn);
    }

    fn insert(&mut self, conn: usize, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn];
        if pending.in_tx {
            pending.staged_inserts.push((table, row));
        } else {
            self.committed[table].push(row.clone());
            pending.last_auto_committed_insert = Some(row);
        }
    }

    fn last_inserted_row(&self, conn: usize) -> Option<SimRow> {
        self.connections[conn].last_auto_committed_insert.clone()
    }

    fn delete(&mut self, conn: usize, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn];
        if pending.in_tx {
            pending
                .staged_inserts
                .retain(|(pending_table, candidate)| !(*pending_table == table && *candidate == row));
            pending.staged_deletes.push((table, row));
        } else {
            self.committed[table].retain(|candidate| *candidate != row);
        }
    }

    fn commit(&mut self, conn: usize) -> Vec<Interaction> {
        let pending = &mut self.connections[conn];
        let inserts = std::mem::take(&mut pending.staged_inserts);
        let deletes = std::mem::take(&mut pending.staged_deletes);
        pending.in_tx = false;
        self.active_writer = None;

        for (table, row) in &deletes {
            self.committed[*table].retain(|candidate| candidate != row);
        }
        for (table, row) in &inserts {
            self.committed[*table].push(row.clone());
        }

        let mut followups = Vec::new();
        for (table, row) in inserts {
            followups.push(Interaction::AssertVisibleFresh { table, row });
        }
        for (table, row) in deletes {
            followups.push(Interaction::AssertMissingFresh { table, row });
        }
        followups
    }

    fn rollback(&mut self, conn: usize) -> Vec<Interaction> {
        let pending = &mut self.connections[conn];
        let touched_tables = pending
            .staged_inserts
            .iter()
            .chain(pending.staged_deletes.iter())
            .map(|(table, _)| *table)
            .collect::<BTreeSet<_>>();
        pending.staged_inserts.clear();
        pending.staged_deletes.clear();
        pending.in_tx = false;
        self.active_writer = None;
        touched_tables
            .into_iter()
            .map(|table| Interaction::AssertRowCountFresh {
                table,
                expected: self.committed[table].len() as u64,
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct ExpectedModel {
    committed: Vec<Vec<SimRow>>,
    connections: Vec<ExpectedConnection>,
    active_writer: Option<usize>,
}

#[derive(Clone, Debug, Default)]
struct ExpectedConnection {
    in_tx: bool,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
}

impl ExpectedModel {
    fn new(table_count: usize, connection_count: usize) -> Self {
        Self {
            committed: vec![Vec::new(); table_count],
            connections: vec![ExpectedConnection::default(); connection_count],
            active_writer: None,
        }
    }

    fn apply(&mut self, interaction: &Interaction) {
        match interaction {
            Interaction::BeginTx { conn } => {
                assert!(self.active_writer.is_none(), "multiple concurrent writers in expected model");
                self.connections[*conn].in_tx = true;
                self.active_writer = Some(*conn);
            }
            Interaction::CommitTx { conn } => {
                assert_eq!(self.active_writer, Some(*conn), "commit by non-owner in expected model");
                let state = &mut self.connections[*conn];
                for (table, row) in state.staged_deletes.drain(..) {
                    self.committed[table].retain(|candidate| *candidate != row);
                }
                for (table, row) in state.staged_inserts.drain(..) {
                    self.committed[table].push(row);
                }
                state.in_tx = false;
                self.active_writer = None;
            }
            Interaction::RollbackTx { conn } => {
                assert_eq!(self.active_writer, Some(*conn), "rollback by non-owner in expected model");
                let state = &mut self.connections[*conn];
                state.staged_inserts.clear();
                state.staged_deletes.clear();
                state.in_tx = false;
                self.active_writer = None;
            }
            Interaction::Insert { conn, table, row } => {
                let state = &mut self.connections[*conn];
                if state.in_tx {
                    state.staged_inserts.push((*table, row.clone()));
                } else {
                    self.committed[*table].push(row.clone());
                }
            }
            Interaction::Delete { conn, table, row } => {
                let state = &mut self.connections[*conn];
                if state.in_tx {
                    state
                        .staged_inserts
                        .retain(|(pending_table, candidate)| !(*pending_table == *table && *candidate == *row));
                    state.staged_deletes.push((*table, row.clone()));
                } else {
                    self.committed[*table].retain(|candidate| *candidate != *row);
                }
            }
            Interaction::AssertVisibleInConnection { .. }
            | Interaction::AssertMissingInConnection { .. }
            | Interaction::AssertVisibleFresh { .. }
            | Interaction::AssertMissingFresh { .. }
            | Interaction::AssertRowCountFresh { .. } => {}
        }
    }

    fn committed_rows(mut self) -> Vec<Vec<SimRow>> {
        for table_rows in &mut self.committed {
            table_rows.sort_by_key(|row| row.id().unwrap_or_default());
        }
        self.committed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use tempfile::tempdir;

    use crate::{
        runner::{rerun_case, run_generated, verify_repeatable_execution},
        seed::DstSeed,
    };

    use super::{
        failure_reason, generate_case, load_bug_artifact, run_case_detailed, run_generated_stream, save_bug_artifact,
        shrink_failure, ColumnKind, ColumnPlan, DatastoreBugArtifact, DatastoreSimulatorCase,
        DatastoreSimulatorSubsystem, Interaction, SchemaPlan, SimRow, SimValue, TablePlan,
    };

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
    fn streamed_runner_supports_long_cases() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        run_generated_stream(DstSeed(1234), 10_000).expect("run long streamed datastore simulator case");
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
                Interaction::Insert { conn, .. }
                | Interaction::Delete { conn, .. }
                | Interaction::AssertVisibleInConnection { conn, .. }
                | Interaction::AssertMissingInConnection { conn, .. } => {
                    if let Some(writer) = owner {
                        assert_eq!(conn, writer, "interaction ran on non-owner while writer open");
                    }
                }
                Interaction::AssertVisibleFresh { .. }
                | Interaction::AssertMissingFresh { .. }
                | Interaction::AssertRowCountFresh { .. } => {}
            }
        }

        assert_eq!(owner, None, "writer left open at end of generated case");
    }

    #[test]
    fn second_writer_fails_fast() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let case = DatastoreSimulatorCase {
            seed: DstSeed(88),
            num_connections: 2,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "locks".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            kind: ColumnKind::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            kind: ColumnKind::String,
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
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            kind: ColumnKind::U64,
                        },
                        ColumnPlan {
                            name: "ok".into(),
                            kind: ColumnKind::Bool,
                        },
                    ],
                    secondary_index_col: Some(1),
                }],
            },
            interactions: vec![Interaction::AssertVisibleFresh {
                table: 0,
                row: SimRow {
                    values: vec![SimValue::U64(7), SimValue::Bool(true)],
                },
            }],
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
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            kind: ColumnKind::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            kind: ColumnKind::String,
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
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::AssertVisibleFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::AssertMissingFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::Insert {
                    conn: 0,
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(2), SimValue::String("two".into())],
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
            num_connections: 1,
            schema: SchemaPlan {
                tables: vec![TablePlan {
                    name: "bugs".into(),
                    columns: vec![
                        ColumnPlan {
                            name: "id".into(),
                            kind: ColumnKind::U64,
                        },
                        ColumnPlan {
                            name: "name".into(),
                            kind: ColumnKind::String,
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
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::AssertVisibleFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
                Interaction::AssertMissingFresh {
                    table: 0,
                    row: SimRow {
                        values: vec![SimValue::U64(1), SimValue::String("one".into())],
                    },
                },
            ],
        }
    }
}
