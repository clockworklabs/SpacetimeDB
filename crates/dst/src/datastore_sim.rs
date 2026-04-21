use std::{collections::BTreeSet, fs, path::Path};

use serde::{Deserialize, Serialize};
use spacetimedb_datastore::{
    execution_context::Workload,
    locking_tx_datastore::{datastore::Locking, MutTxId},
    traits::{IsolationLevel, MutTx, MutTxDatastore, Tx},
};
use spacetimedb_execution::Datastore as _;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    table_name::TableName,
};

use crate::{
    datastore::bootstrap_datastore,
    seed::{DstRng, DstSeed},
    subsystem::{DstSubsystem, RunRecord},
    trace::Trace,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreSimulatorCase {
    pub seed: DstSeed,
    pub num_connections: usize,
    pub schema: SchemaPlan,
    pub interactions: Vec<Interaction>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchemaPlan {
    pub tables: Vec<TablePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TablePlan {
    pub name: String,
    pub columns: Vec<ColumnPlan>,
    pub secondary_index_col: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ColumnPlan {
    pub name: String,
    pub kind: ColumnKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ColumnKind {
    U64,
    String,
    Bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimRow {
    pub values: Vec<SimValue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SimValue {
    U64(u64),
    String(String),
    Bool(bool),
}

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DatastoreSimulatorEvent {
    Executed(Interaction),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreSimulatorOutcome {
    pub final_row_counts: Vec<u64>,
    pub final_rows: Vec<Vec<SimRow>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatastoreExecutionFailure {
    pub step_index: usize,
    pub reason: String,
    pub interaction: Interaction,
}

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

pub fn generate_case(seed: DstSeed) -> DatastoreSimulatorCase {
    let mut rng = seed.fork(17).rng();
    let num_connections = rng.index(3) + 1;
    let schema = generate_schema(&mut rng);
    let interactions = generate_interactions(seed, &schema, num_connections, &mut rng);
    DatastoreSimulatorCase {
        seed,
        num_connections,
        schema,
        interactions,
    }
}

pub fn run_case_detailed(
    case: &DatastoreSimulatorCase,
) -> Result<
    RunRecord<DatastoreSimulatorCase, DatastoreSimulatorEvent, DatastoreSimulatorOutcome>,
    DatastoreExecutionFailure,
> {
    let datastore = bootstrap_datastore().map_err(|err| failure_without_step(format!("bootstrap failed: {err}")))?;
    let table_ids = install_schema(&datastore, &case.schema)
        .map_err(|err| failure_without_step(format!("schema install failed: {err}")))?;
    let mut trace = Trace::default();
    let mut connections: Vec<Option<MutTxId>> = (0..case.num_connections).map(|_| None).collect();

    for (step_index, interaction) in case.interactions.iter().cloned().enumerate() {
        trace.push(DatastoreSimulatorEvent::Executed(interaction.clone()));
        execute_interaction(&datastore, &table_ids, &mut connections, &interaction).map_err(|reason| {
            DatastoreExecutionFailure {
                step_index,
                reason,
                interaction,
            }
        })?;
    }

    for tx in &mut connections {
        if let Some(tx) = tx.take() {
            let _ = datastore.rollback_mut_tx(tx);
        }
    }

    let outcome = collect_outcome(&datastore, &table_ids)
        .map_err(|err| failure_without_step(format!("collect outcome failed: {err}")))?;
    let expected_rows = expected_committed_rows(case);
    if outcome.final_rows != expected_rows {
        return Err(failure_without_step(format!(
            "final datastore state mismatch: expected={expected_rows:?} actual={:?}",
            outcome.final_rows
        )));
    }

    Ok(RunRecord {
        subsystem: DatastoreSimulatorSubsystem::name(),
        seed: case.seed,
        case: case.clone(),
        trace: Some(trace),
        outcome,
    })
}

pub fn save_case(path: impl AsRef<Path>, case: &DatastoreSimulatorCase) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(case)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn load_case(path: impl AsRef<Path>) -> anyhow::Result<DatastoreSimulatorCase> {
    let body = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}

pub fn failure_reason(case: &DatastoreSimulatorCase) -> anyhow::Result<String> {
    match run_case_detailed(case) {
        Ok(_) => anyhow::bail!("case did not fail"),
        Err(failure) => Ok(failure.reason),
    }
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

fn generate_interactions(
    seed: DstSeed,
    schema: &SchemaPlan,
    num_connections: usize,
    rng: &mut DstRng,
) -> Vec<Interaction> {
    let mut plan = Vec::new();
    let mut model = GenerationModel::new(schema, num_connections, seed);
    let target_ops = 24 + rng.index(24);

    while plan.len() < target_ops {
        let conn = model.open_tx_conn().unwrap_or_else(|| rng.index(num_connections));

        if !model.connections[conn].in_tx && model.open_tx_conn().is_none() && rng.index(100) < 20 {
            model.connections[conn].in_tx = true;
            plan.push(Interaction::BeginTx { conn });
            continue;
        }

        if model.connections[conn].in_tx && rng.index(100) < 15 {
            let followups = model.commit(conn);
            plan.push(Interaction::CommitTx { conn });
            plan.extend(followups);
            continue;
        }

        if model.connections[conn].in_tx && rng.index(100) < 10 {
            let followups = model.rollback(conn);
            plan.push(Interaction::RollbackTx { conn });
            plan.extend(followups);
            continue;
        }

        let table = rng.index(schema.tables.len());
        let visible_rows = model.visible_rows(conn, table);
        let choose_insert = visible_rows.is_empty() || rng.index(100) < 65;
        if choose_insert {
            let row = model.make_row(rng, table);
            model.insert(conn, table, row.clone());
            plan.push(Interaction::Insert {
                conn,
                table,
                row: row.clone(),
            });
            plan.push(Interaction::AssertVisibleInConnection { conn, table, row });
            if !model.connections[conn].in_tx {
                let row = model.last_inserted_row(conn).expect("tracked auto-commit insert");
                plan.push(Interaction::AssertVisibleFresh { table, row });
            }
        } else {
            let row = visible_rows[rng.index(visible_rows.len())].clone();
            model.delete(conn, table, row.clone());
            plan.push(Interaction::Delete {
                conn,
                table,
                row: row.clone(),
            });
            plan.push(Interaction::AssertMissingInConnection {
                conn,
                table,
                row: row.clone(),
            });
            if !model.connections[conn].in_tx {
                plan.push(Interaction::AssertMissingFresh { table, row });
            }
        }
    }

    for conn in 0..num_connections {
        if model.connections[conn].in_tx {
            let followups = model.commit(conn);
            plan.push(Interaction::CommitTx { conn });
            plan.extend(followups);
        }
    }

    plan
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
    connections: &mut [Option<MutTxId>],
    interaction: &Interaction,
) -> Result<(), String> {
    match interaction {
        Interaction::BeginTx { conn } => {
            let slot = connections
                .get_mut(*conn)
                .ok_or_else(|| format!("connection {conn} out of range"))?;
            if slot.is_some() {
                return Err(format!("connection {conn} already has open transaction"));
            }
            *slot = Some(datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests));
        }
        Interaction::CommitTx { conn } => {
            let tx = connections
                .get_mut(*conn)
                .ok_or_else(|| format!("connection {conn} out of range"))?
                .take()
                .ok_or_else(|| format!("connection {conn} has no transaction to commit"))?;
            datastore
                .commit_mut_tx(tx)
                .map_err(|err| format!("commit failed on connection {conn}: {err}"))?;
        }
        Interaction::RollbackTx { conn } => {
            let tx = connections
                .get_mut(*conn)
                .ok_or_else(|| format!("connection {conn} out of range"))?
                .take()
                .ok_or_else(|| format!("connection {conn} has no transaction to rollback"))?;
            let _ = datastore.rollback_mut_tx(tx);
        }
        Interaction::Insert { conn, table, row } => {
            with_mut_tx(
                datastore,
                table_ids,
                connections,
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
                connections,
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
            let found = if let Some(Some(tx)) = connections.get(*conn) {
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
            let found = if let Some(Some(tx)) = connections.get(*conn) {
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
    connections: &mut [Option<MutTxId>],
    conn: usize,
    table: usize,
    mut f: impl FnMut(&Locking, TableId, &mut MutTxId) -> Result<(), String>,
) -> Result<(), String> {
    let table_id = *table_ids
        .get(table)
        .ok_or_else(|| format!("table {table} out of range"))?;
    let slot = connections
        .get_mut(conn)
        .ok_or_else(|| format!("connection {conn} out of range"))?;

    match slot {
        Some(tx) => f(datastore, table_id, tx),
        None => {
            let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
            f(datastore, table_id, &mut tx)?;
            datastore
                .commit_mut_tx(tx)
                .map_err(|err| format!("auto-commit failed on connection {conn}: {err}"))?;
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

fn expected_committed_rows(case: &DatastoreSimulatorCase) -> Vec<Vec<SimRow>> {
    let mut model = ExpectedModel::new(case.schema.tables.len(), case.num_connections);
    for interaction in &case.interactions {
        model.apply(interaction);
    }
    let mut rows = model.committed;
    for table_rows in &mut rows {
        table_rows.sort_by_key(|row| row.id().unwrap_or_default());
    }
    rows
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

#[derive(Clone, Debug)]
struct GenerationModel {
    schema: SchemaPlan,
    connections: Vec<PendingConnection>,
    committed: Vec<Vec<SimRow>>,
    next_ids: Vec<u64>,
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

    fn open_tx_conn(&self) -> Option<usize> {
        self.connections.iter().position(|conn| conn.in_tx)
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
        }
    }

    fn apply(&mut self, interaction: &Interaction) {
        match interaction {
            Interaction::BeginTx { conn } => self.connections[*conn].in_tx = true,
            Interaction::CommitTx { conn } => {
                let state = &mut self.connections[*conn];
                for (table, row) in state.staged_deletes.drain(..) {
                    self.committed[table].retain(|candidate| *candidate != row);
                }
                for (table, row) in state.staged_inserts.drain(..) {
                    self.committed[table].push(row);
                }
                state.in_tx = false;
            }
            Interaction::RollbackTx { conn } => {
                let state = &mut self.connections[*conn];
                state.staged_inserts.clear();
                state.staged_deletes.clear();
                state.in_tx = false;
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
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use crate::{
        runner::{rerun_case, run_generated, verify_repeatable_execution},
        seed::DstSeed,
    };

    use super::{
        failure_reason, run_case_detailed, ColumnKind, ColumnPlan, DatastoreSimulatorCase, DatastoreSimulatorSubsystem,
        Interaction, SchemaPlan, SimRow, SimValue, TablePlan,
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
