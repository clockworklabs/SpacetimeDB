use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    schema::{distinct_value_for_type, generate_value_for_type, ColumnPlan, SchemaPlan, SimRow},
    seed::{DstRng, DstSeed},
};

use super::{ExpectedResult, TableOperation, TableWorkloadInteraction};

/// Generator-side model of committed rows plus per-connection pending writes.
///
/// This model is used only while producing interactions. It lets the planner
/// pick valid deletes, synthesize visibility checks, and enforce the
/// single-writer discipline before the real target executes anything.
#[derive(Clone, Debug)]
pub(crate) struct GenerationModel {
    pub(crate) schema: SchemaPlan,
    pub(crate) connections: Vec<PendingConnection>,
    committed: Vec<Vec<SimRow>>,
    next_ids: Vec<u64>,
    active_writer: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingConnection {
    pub(crate) in_tx: bool,
    read_snapshot: Option<Vec<Vec<SimRow>>>,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
}

impl GenerationModel {
    pub(crate) fn new(schema: &SchemaPlan, num_connections: usize, seed: DstSeed) -> Self {
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

    pub(crate) fn make_row(&mut self, rng: &mut DstRng, table: usize) -> SimRow {
        let table_plan = &self.schema.tables[table];
        let id = self.next_ids[table];
        self.next_ids[table] = self.next_ids[table].wrapping_add(1).max(1);
        let mut values = vec![AlgebraicValue::U64(id)];
        for (idx, col) in table_plan.columns.iter().enumerate().skip(1) {
            values.push(generate_value_for_type(rng, &col.ty, idx));
        }
        SimRow { values }
    }

    pub(crate) fn visible_rows(&self, conn: usize, table: usize) -> Vec<SimRow> {
        if let Some(snapshot) = &self.connections[conn].read_snapshot {
            return snapshot[table].clone();
        }
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

    pub(crate) fn absent_row(&mut self, rng: &mut DstRng, conn: usize, table: usize) -> SimRow {
        let mut row = self.make_row(rng, table);
        while self.visible_rows(conn, table).iter().any(|candidate| candidate == &row) {
            row = self.make_row(rng, table);
        }
        row
    }

    pub(crate) fn unique_key_conflict_row(&self, rng: &mut DstRng, table: usize, source: &SimRow) -> Option<SimRow> {
        let table_plan = &self.schema.tables[table];
        let value_count = source.values.len().min(table_plan.columns.len());
        if value_count <= 1 {
            return None;
        }

        let col_idx = 1 + rng.index(value_count - 1);
        let mut row = source.clone();
        row.values[col_idx] = distinct_value_for_type(&table_plan.columns[col_idx].ty, &row.values[col_idx]);
        Some(row)
    }

    pub(crate) fn active_writer(&self) -> Option<usize> {
        self.active_writer
    }

    pub(crate) fn has_read_tx(&self, conn: usize) -> bool {
        self.connections[conn].read_snapshot.is_some()
    }

    pub(crate) fn any_read_tx(&self) -> bool {
        self.connections
            .iter()
            .any(|connection| connection.read_snapshot.is_some())
    }

    pub(crate) fn begin_read_tx(&mut self, conn: usize) {
        let pending = &mut self.connections[conn];
        assert!(!pending.in_tx, "connection already has write transaction");
        assert!(
            pending.read_snapshot.is_none(),
            "connection already has read transaction"
        );
        pending.read_snapshot = Some(self.committed.clone());
    }

    pub(crate) fn release_read_tx(&mut self, conn: usize) {
        assert!(
            self.connections[conn].read_snapshot.take().is_some(),
            "connection has no read transaction"
        );
    }

    pub(crate) fn begin_tx(&mut self, conn: usize) {
        assert!(self.active_writer.is_none(), "single writer already active");
        let pending = &mut self.connections[conn];
        assert!(!pending.in_tx, "connection already in transaction");
        assert!(
            pending.read_snapshot.is_none(),
            "connection already has read transaction"
        );
        pending.in_tx = true;
        self.active_writer = Some(conn);
    }

    pub(crate) fn insert(&mut self, conn: usize, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn];
        if pending.in_tx {
            pending.staged_inserts.push((table, row));
        } else {
            self.committed[table].push(row);
        }
    }

    pub(crate) fn batch_insert(&mut self, conn: usize, table: usize, rows: &[SimRow]) {
        for row in rows {
            self.insert(conn, table, row.clone());
        }
    }

    pub(crate) fn delete(&mut self, conn: usize, table: usize, row: SimRow) {
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

    pub(crate) fn batch_delete(&mut self, conn: usize, table: usize, rows: &[SimRow]) {
        for row in rows {
            self.delete(conn, table, row.clone());
        }
    }

    pub(crate) fn commit(&mut self, conn: usize) {
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
    }

    pub(crate) fn rollback(&mut self, conn: usize) {
        let pending = &mut self.connections[conn];
        pending.staged_inserts.clear();
        pending.staged_deletes.clear();
        pending.in_tx = false;
        self.active_writer = None;
    }

    pub(crate) fn add_column(&mut self, table: usize, column: ColumnPlan, default: AlgebraicValue) {
        self.schema.tables[table].columns.push(column);
        for row in &mut self.committed[table] {
            row.values.push(default.clone());
        }
        for connection in &mut self.connections {
            for (pending_table, row) in connection
                .staged_inserts
                .iter_mut()
                .chain(connection.staged_deletes.iter_mut())
            {
                if *pending_table == table {
                    row.values.push(default.clone());
                }
            }
            if let Some(snapshot) = &mut connection.read_snapshot {
                for row in &mut snapshot[table] {
                    row.values.push(default.clone());
                }
            }
        }
    }

    pub(crate) fn add_index(&mut self, table: usize, cols: Vec<u16>) {
        let indexes = &mut self.schema.tables[table].extra_indexes;
        if !indexes.contains(&cols) {
            indexes.push(cols);
        }
    }
}

/// Replay model for the expected final committed state of a table workload.
///
/// Target property runtimes apply every table interaction here in parallel with
/// real target execution, then compare the collected target outcome against this
/// model at the end of the run.
#[derive(Clone, Debug)]
pub struct ExpectedModel {
    committed: Vec<Vec<SimRow>>,
    connections: Vec<ExpectedConnection>,
    active_writer: Option<usize>,
}

#[derive(Clone, Debug, Default)]
struct ExpectedConnection {
    in_tx: bool,
    read_snapshot: Option<Vec<Vec<SimRow>>>,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
}

impl ExpectedModel {
    pub fn new(table_count: usize, connection_count: usize) -> Self {
        Self {
            committed: vec![Vec::new(); table_count],
            connections: vec![ExpectedConnection::default(); connection_count],
            active_writer: None,
        }
    }

    pub fn apply(&mut self, interaction: &TableWorkloadInteraction) {
        if !matches!(interaction.expected, ExpectedResult::Ok) {
            return;
        }
        match &interaction.op {
            TableOperation::BeginTx { conn } => {
                assert!(
                    self.active_writer.is_none(),
                    "multiple concurrent writers in expected model"
                );
                self.connections[*conn].in_tx = true;
                self.active_writer = Some(*conn);
            }
            TableOperation::BeginReadTx { conn } => {
                let state = &mut self.connections[*conn];
                assert!(!state.in_tx, "read tx started while write tx is open");
                assert!(state.read_snapshot.is_none(), "nested read tx in expected model");
                state.read_snapshot = Some(self.committed.clone());
            }
            TableOperation::ReleaseReadTx { conn } => {
                assert!(
                    self.connections[*conn].read_snapshot.take().is_some(),
                    "release read tx without open read tx"
                );
            }
            TableOperation::CommitTx { conn } => {
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
            TableOperation::RollbackTx { conn } => {
                assert_eq!(
                    self.active_writer,
                    Some(*conn),
                    "rollback by non-owner in expected model"
                );
                let state = &mut self.connections[*conn];
                state.staged_inserts.clear();
                state.staged_deletes.clear();
                state.in_tx = false;
                self.active_writer = None;
            }
            TableOperation::Insert { conn, table, row } => {
                self.insert(*conn, *table, row.clone());
            }
            TableOperation::Delete { conn, table, row } => {
                self.delete(*conn, *table, row.clone());
            }
            TableOperation::BatchInsert { conn, table, rows } => {
                for row in rows {
                    self.insert(*conn, *table, row.clone());
                }
            }
            TableOperation::BatchDelete { conn, table, rows } => {
                for row in rows {
                    self.delete(*conn, *table, row.clone());
                }
            }
            TableOperation::Reinsert { conn, table, row } => {
                self.delete(*conn, *table, row.clone());
                self.insert(*conn, *table, row.clone());
            }
            TableOperation::AddColumn {
                table,
                column: _,
                default,
                ..
            } => {
                self.add_column(*table, default.clone());
            }
            TableOperation::AddIndex { .. } => {}
            TableOperation::ExactDuplicateInsert { .. }
            | TableOperation::UniqueKeyConflictInsert { .. }
            | TableOperation::DeleteMissing { .. }
            | TableOperation::BeginTxConflict { .. }
            | TableOperation::WriteConflictInsert { .. }
            | TableOperation::PointLookup { .. }
            | TableOperation::PredicateCount { .. }
            | TableOperation::RangeScan { .. }
            | TableOperation::FullScan { .. } => {}
        }
    }

    pub fn visible_rows(&self, conn: usize, table: usize) -> Vec<SimRow> {
        if let Some(snapshot) = &self.connections[conn].read_snapshot {
            return snapshot[table].clone();
        }
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

    pub fn lookup_by_id(&self, conn: usize, table: usize, id: u64) -> Option<SimRow> {
        self.visible_rows(conn, table)
            .into_iter()
            .find(|row| row.id() == Some(id))
    }

    pub fn predicate_count(&self, conn: usize, table: usize, col: u16, value: &AlgebraicValue) -> usize {
        self.visible_rows(conn, table)
            .into_iter()
            .filter(|row| row.values.get(col as usize) == Some(value))
            .count()
    }

    pub fn range_scan(
        &self,
        conn: usize,
        table: usize,
        cols: &[u16],
        lower: &Bound<AlgebraicValue>,
        upper: &Bound<AlgebraicValue>,
    ) -> Vec<SimRow> {
        let mut rows = self
            .visible_rows(conn, table)
            .into_iter()
            .filter(|row| {
                let key = row.project_key(cols).to_algebraic_value();
                bound_contains_lower(lower, &key) && bound_contains_upper(upper, &key)
            })
            .collect::<Vec<_>>();
        rows.sort_by(|lhs, rhs| {
            lhs.project_key(cols)
                .to_algebraic_value()
                .cmp(&rhs.project_key(cols).to_algebraic_value())
                .then_with(|| lhs.values.cmp(&rhs.values))
        });
        rows
    }

    pub fn committed_rows(mut self) -> Vec<Vec<SimRow>> {
        for table_rows in &mut self.committed {
            table_rows.sort_by_key(|row| row.id().unwrap_or_default());
        }
        self.committed
    }

    fn insert(&mut self, conn: usize, table: usize, row: SimRow) {
        let state = &mut self.connections[conn];
        if state.in_tx {
            state.staged_inserts.push((table, row));
        } else {
            self.committed[table].push(row);
        }
    }

    fn delete(&mut self, conn: usize, table: usize, row: SimRow) {
        let state = &mut self.connections[conn];
        if state.in_tx {
            state
                .staged_inserts
                .retain(|(pending_table, candidate)| !(*pending_table == table && *candidate == row));
            state.staged_deletes.push((table, row));
        } else {
            self.committed[table].retain(|candidate| *candidate != row);
        }
    }

    fn add_column(&mut self, table: usize, default: AlgebraicValue) {
        for row in &mut self.committed[table] {
            row.values.push(default.clone());
        }
        for connection in &mut self.connections {
            for (pending_table, row) in connection
                .staged_inserts
                .iter_mut()
                .chain(connection.staged_deletes.iter_mut())
            {
                if *pending_table == table {
                    row.values.push(default.clone());
                }
            }
            if let Some(snapshot) = &mut connection.read_snapshot {
                for row in &mut snapshot[table] {
                    row.values.push(default.clone());
                }
            }
        }
    }
}

fn bound_contains_lower(bound: &Bound<AlgebraicValue>, key: &AlgebraicValue) -> bool {
    match bound {
        Bound::Included(value) => key >= value,
        Bound::Excluded(value) => key > value,
        Bound::Unbounded => true,
    }
}

fn bound_contains_upper(bound: &Bound<AlgebraicValue>, key: &AlgebraicValue) -> bool {
    match bound {
        Bound::Included(value) => key <= value,
        Bound::Excluded(value) => key < value,
        Bound::Unbounded => true,
    }
}
