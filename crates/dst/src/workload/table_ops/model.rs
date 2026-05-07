use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    client::SessionId,
    schema::{distinct_value_for_type, generate_value_for_type, ColumnPlan, SchemaPlan, SimRow},
    seed::{DstRng, DstSeed},
};

use super::{TableErrorKind, TableOperation};

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
    active_writer: Option<SessionId>,
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

    pub(crate) fn visible_rows(&self, conn: SessionId, table: usize) -> Vec<SimRow> {
        let conn_idx = conn.as_index();
        if let Some(snapshot) = &self.connections[conn_idx].read_snapshot {
            return snapshot[table].clone();
        }
        let mut rows = self.committed[table].clone();
        let pending = &self.connections[conn_idx];
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

    pub(crate) fn absent_row(&mut self, rng: &mut DstRng, conn: SessionId, table: usize) -> SimRow {
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

    pub(crate) fn active_writer(&self) -> Option<SessionId> {
        self.active_writer
    }

    pub(crate) fn has_read_tx(&self, conn: SessionId) -> bool {
        self.connections[conn.as_index()].read_snapshot.is_some()
    }

    pub(crate) fn any_read_tx(&self) -> bool {
        self.connections
            .iter()
            .any(|connection| connection.read_snapshot.is_some())
    }

    pub(crate) fn begin_read_tx(&mut self, conn: SessionId) {
        let pending = &mut self.connections[conn.as_index()];
        assert!(!pending.in_tx, "connection already has write transaction");
        assert!(
            pending.read_snapshot.is_none(),
            "connection already has read transaction"
        );
        pending.read_snapshot = Some(self.committed.clone());
    }

    pub(crate) fn release_read_tx(&mut self, conn: SessionId) {
        assert!(
            self.connections[conn.as_index()].read_snapshot.take().is_some(),
            "connection has no read transaction"
        );
    }

    pub(crate) fn begin_tx(&mut self, conn: SessionId) {
        assert!(self.active_writer.is_none(), "single writer already active");
        let pending = &mut self.connections[conn.as_index()];
        assert!(!pending.in_tx, "connection already in transaction");
        assert!(
            pending.read_snapshot.is_none(),
            "connection already has read transaction"
        );
        pending.in_tx = true;
        self.active_writer = Some(conn);
    }

    pub(crate) fn insert(&mut self, conn: SessionId, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn.as_index()];
        if pending.in_tx {
            pending.staged_inserts.push((table, row));
        } else {
            self.committed[table].push(row);
        }
    }

    pub(crate) fn batch_insert(&mut self, conn: SessionId, table: usize, rows: &[SimRow]) {
        for row in rows {
            self.insert(conn, table, row.clone());
        }
    }

    pub(crate) fn delete(&mut self, conn: SessionId, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn.as_index()];
        if pending.in_tx {
            pending
                .staged_inserts
                .retain(|(pending_table, candidate)| !(*pending_table == table && *candidate == row));
            pending.staged_deletes.push((table, row));
        } else {
            self.committed[table].retain(|candidate| *candidate != row);
        }
    }

    pub(crate) fn batch_delete(&mut self, conn: SessionId, table: usize, rows: &[SimRow]) {
        for row in rows {
            self.delete(conn, table, row.clone());
        }
    }

    pub(crate) fn commit(&mut self, conn: SessionId) {
        let pending = &mut self.connections[conn.as_index()];
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

    pub(crate) fn rollback(&mut self, conn: SessionId) {
        let pending = &mut self.connections[conn.as_index()];
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

/// Replay model used as the oracle for table workload properties.
///
/// Target property runtimes apply every table interaction here in parallel with
/// real target execution, then compare the collected target outcome against this
/// model at the end of the run.
#[derive(Clone, Debug)]
pub struct TableOracle {
    committed: Vec<Vec<SimRow>>,
    connections: Vec<ExpectedConnection>,
    active_writer: Option<SessionId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PredictedOutcome {
    Applied,
    NoMutation {
        subject: Option<(SessionId, usize)>,
    },
    Error {
        kind: TableErrorKind,
        subject: Option<(SessionId, usize)>,
    },
}

#[derive(Clone, Debug, Default)]
struct ExpectedConnection {
    in_tx: bool,
    read_snapshot: Option<Vec<Vec<SimRow>>>,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
}

impl TableOracle {
    pub fn new(table_count: usize, connection_count: usize) -> Self {
        Self {
            committed: vec![Vec::new(); table_count],
            connections: vec![ExpectedConnection::default(); connection_count],
            active_writer: None,
        }
    }

    pub fn predict(&self, op: &TableOperation) -> Result<PredictedOutcome, String> {
        match op {
            TableOperation::BeginTx { conn } => {
                self.ensure_connection(*conn)?;
                if self.connections[conn.as_index()].read_snapshot.is_some() {
                    return Err(format!("connection {conn} cannot begin write tx with open read tx"));
                }
                if self.connections[conn.as_index()].in_tx {
                    return Err(format!("connection {conn} already has open write tx"));
                }
                if self.active_writer.is_some()
                    || self.connections.iter().any(|connection| connection.read_snapshot.is_some())
                {
                    return Ok(PredictedOutcome::Error {
                        kind: TableErrorKind::WriteConflict,
                        subject: None,
                    });
                }
                Ok(PredictedOutcome::Applied)
            }
            TableOperation::BeginReadTx { conn } => {
                self.ensure_connection(*conn)?;
                let state = &self.connections[conn.as_index()];
                if state.in_tx || state.read_snapshot.is_some() {
                    return Err(format!("connection {conn} cannot begin read tx in current state"));
                }
                Ok(PredictedOutcome::Applied)
            }
            TableOperation::ReleaseReadTx { conn } => {
                self.ensure_connection(*conn)?;
                if self.connections[conn.as_index()].read_snapshot.is_none() {
                    return Err(format!("connection {conn} has no read tx to release"));
                }
                Ok(PredictedOutcome::Applied)
            }
            TableOperation::CommitTx { conn } | TableOperation::RollbackTx { conn } => {
                self.ensure_connection(*conn)?;
                if self.active_writer != Some(*conn) || !self.connections[conn.as_index()].in_tx {
                    return Err(format!("connection {conn} does not own an open write tx"));
                }
                Ok(PredictedOutcome::Applied)
            }
            TableOperation::InsertRows { conn, table, rows } => self.predict_insert_rows(*conn, *table, rows),
            TableOperation::DeleteRows { conn, table, rows } => self.predict_delete_rows(*conn, *table, rows),
            TableOperation::AddColumn { .. } | TableOperation::AddIndex { .. } => Ok(PredictedOutcome::Applied),
            TableOperation::PointLookup { .. }
            | TableOperation::PredicateCount { .. }
            | TableOperation::RangeScan { .. }
            | TableOperation::FullScan { .. } => Ok(PredictedOutcome::NoMutation { subject: None }),
        }
    }

    pub fn apply(&mut self, op: &TableOperation) {
        match op {
            TableOperation::BeginTx { conn } => {
                assert!(
                    self.active_writer.is_none(),
                    "multiple concurrent writers in table oracle"
                );
                self.connections[conn.as_index()].in_tx = true;
                self.active_writer = Some(*conn);
            }
            TableOperation::BeginReadTx { conn } => {
                let state = &mut self.connections[conn.as_index()];
                assert!(!state.in_tx, "read tx started while write tx is open");
                assert!(state.read_snapshot.is_none(), "nested read tx in table oracle");
                state.read_snapshot = Some(self.committed.clone());
            }
            TableOperation::ReleaseReadTx { conn } => {
                assert!(
                    self.connections[conn.as_index()].read_snapshot.take().is_some(),
                    "release read tx without open read tx"
                );
            }
            TableOperation::CommitTx { conn } => {
                assert_eq!(self.active_writer, Some(*conn), "commit by non-owner in table oracle");
                let state = &mut self.connections[conn.as_index()];
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
                assert_eq!(self.active_writer, Some(*conn), "rollback by non-owner in table oracle");
                let state = &mut self.connections[conn.as_index()];
                state.staged_inserts.clear();
                state.staged_deletes.clear();
                state.in_tx = false;
                self.active_writer = None;
            }
            TableOperation::InsertRows { conn, table, rows } => self.insert_rows(*conn, *table, rows),
            TableOperation::DeleteRows { conn, table, rows } => self.delete_rows(*conn, *table, rows),
            TableOperation::AddColumn {
                table,
                column: _,
                default,
                ..
            } => {
                self.add_column(*table, default.clone());
            }
            TableOperation::AddIndex { .. } => {}
            TableOperation::PointLookup { .. }
            | TableOperation::PredicateCount { .. }
            | TableOperation::RangeScan { .. }
            | TableOperation::FullScan { .. } => {}
        }
    }

    fn predict_insert_rows(&self, conn: SessionId, table: usize, rows: &[SimRow]) -> Result<PredictedOutcome, String> {
        if let Some(outcome) = self.predict_write_access(conn, table)? {
            return Ok(outcome);
        }

        let mut visible = self.visible_rows(conn, table);
        let mut mutates = false;
        for row in rows {
            let Some(id) = row.id() else {
                return Err(format!("insert row for table {table} is missing primary id: {row:?}"));
            };
            match visible.iter().find(|candidate| candidate.id() == Some(id)) {
                Some(existing) if existing == row => {}
                Some(_) => {
                    return Ok(PredictedOutcome::Error {
                        kind: TableErrorKind::UniqueConstraintViolation,
                        subject: Some((conn, table)),
                    });
                }
                None => {
                    mutates = true;
                    visible.push(row.clone());
                }
            }
        }

        if mutates {
            Ok(PredictedOutcome::Applied)
        } else {
            Ok(PredictedOutcome::NoMutation {
                subject: Some((conn, table)),
            })
        }
    }

    fn predict_delete_rows(&self, conn: SessionId, table: usize, rows: &[SimRow]) -> Result<PredictedOutcome, String> {
        if let Some(outcome) = self.predict_write_access(conn, table)? {
            return Ok(outcome);
        }

        let mut visible = self.visible_rows(conn, table);
        for row in rows {
            let Some(idx) = visible.iter().position(|candidate| candidate == row) else {
                return Ok(PredictedOutcome::Error {
                    kind: TableErrorKind::MissingRow,
                    subject: Some((conn, table)),
                });
            };
            visible.remove(idx);
        }

        Ok(PredictedOutcome::Applied)
    }

    fn predict_write_access(&self, conn: SessionId, table: usize) -> Result<Option<PredictedOutcome>, String> {
        self.ensure_connection(conn)?;
        self.ensure_table(table)?;
        if self.connections[conn.as_index()].read_snapshot.is_some() {
            return Err(format!("connection {conn} cannot write while read tx is open"));
        }
        if let Some(owner) = self.active_writer
            && owner != conn
        {
            return Ok(Some(PredictedOutcome::Error {
                kind: TableErrorKind::WriteConflict,
                subject: None,
            }));
        }
        Ok(None)
    }

    fn ensure_connection(&self, conn: SessionId) -> Result<(), String> {
        self.connections
            .get(conn.as_index())
            .map(|_| ())
            .ok_or_else(|| format!("connection {conn} out of range"))
    }

    fn ensure_table(&self, table: usize) -> Result<(), String> {
        self.committed
            .get(table)
            .map(|_| ())
            .ok_or_else(|| format!("table {table} out of range"))
    }

    pub fn visible_rows(&self, conn: SessionId, table: usize) -> Vec<SimRow> {
        let conn_idx = conn.as_index();
        if let Some(snapshot) = &self.connections[conn_idx].read_snapshot {
            return snapshot[table].clone();
        }
        let mut rows = self.committed[table].clone();
        let pending = &self.connections[conn_idx];
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

    pub fn lookup_by_id(&self, conn: SessionId, table: usize, id: u64) -> Option<SimRow> {
        self.visible_rows(conn, table)
            .into_iter()
            .find(|row| row.id() == Some(id))
    }

    pub fn predicate_count(&self, conn: SessionId, table: usize, col: u16, value: &AlgebraicValue) -> usize {
        self.visible_rows(conn, table)
            .into_iter()
            .filter(|row| row.values.get(col as usize) == Some(value))
            .count()
    }

    pub fn range_scan(
        &self,
        conn: SessionId,
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

    fn insert(&mut self, conn: SessionId, table: usize, row: SimRow) {
        let state = &mut self.connections[conn.as_index()];
        if state.in_tx {
            state.staged_inserts.push((table, row));
        } else {
            self.committed[table].push(row);
        }
    }

    fn insert_rows(&mut self, conn: SessionId, table: usize, rows: &[SimRow]) {
        for row in rows {
            if self
                .visible_rows(conn, table)
                .into_iter()
                .any(|candidate| candidate == *row)
            {
                continue;
            }
            self.insert(conn, table, row.clone());
        }
    }

    fn delete(&mut self, conn: SessionId, table: usize, row: SimRow) {
        let state = &mut self.connections[conn.as_index()];
        if state.in_tx {
            state
                .staged_inserts
                .retain(|(pending_table, candidate)| !(*pending_table == table && *candidate == row));
            state.staged_deletes.push((table, row));
        } else {
            self.committed[table].retain(|candidate| *candidate != row);
        }
    }

    fn delete_rows(&mut self, conn: SessionId, table: usize, rows: &[SimRow]) {
        for row in rows {
            self.delete(conn, table, row.clone());
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

#[cfg(test)]
mod tests {
    use spacetimedb_sats::AlgebraicValue;

    use crate::{client::SessionId, schema::SimRow};

    use super::{PredictedOutcome, TableErrorKind, TableOperation, TableOracle};

    fn row(id: u64) -> SimRow {
        SimRow {
            values: vec![AlgebraicValue::U64(id)],
        }
    }

    #[test]
    fn write_conflict_prediction_does_not_request_blocking_visibility_check() {
        let owner = SessionId::from_index(0);
        let contender = SessionId::from_index(1);
        let mut oracle = TableOracle::new(1, 2);
        oracle.apply(&TableOperation::BeginTx { conn: owner });

        let prediction = oracle
            .predict(&TableOperation::InsertRows {
                conn: contender,
                table: 0,
                rows: vec![row(1)],
            })
            .unwrap();

        assert_eq!(
            prediction,
            PredictedOutcome::Error {
                kind: TableErrorKind::WriteConflict,
                subject: None,
            }
        );
    }

    #[test]
    fn exact_duplicate_insert_is_predicted_as_no_mutation() {
        let conn = SessionId::from_index(0);
        let mut oracle = TableOracle::new(1, 1);
        oracle.apply(&TableOperation::InsertRows {
            conn,
            table: 0,
            rows: vec![row(1)],
        });

        let prediction = oracle
            .predict(&TableOperation::InsertRows {
                conn,
                table: 0,
                rows: vec![row(1)],
            })
            .unwrap();

        assert_eq!(
            prediction,
            PredictedOutcome::NoMutation {
                subject: Some((conn, 0)),
            }
        );
    }

    #[test]
    fn same_id_different_row_is_predicted_as_unique_constraint_violation() {
        let conn = SessionId::from_index(0);
        let mut oracle = TableOracle::new(1, 1);
        oracle.apply(&TableOperation::InsertRows {
            conn,
            table: 0,
            rows: vec![SimRow {
                values: vec![AlgebraicValue::U64(1), AlgebraicValue::U64(10)],
            }],
        });

        let prediction = oracle
            .predict(&TableOperation::InsertRows {
                conn,
                table: 0,
                rows: vec![SimRow {
                    values: vec![AlgebraicValue::U64(1), AlgebraicValue::U64(11)],
                }],
            })
            .unwrap();

        assert_eq!(
            prediction,
            PredictedOutcome::Error {
                kind: TableErrorKind::UniqueConstraintViolation,
                subject: Some((conn, 0)),
            }
        );
    }
}
