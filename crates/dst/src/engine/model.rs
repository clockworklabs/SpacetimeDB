use super::workload::{
    normalize_rows, CommitDelta, CountState, Interaction, Observation, Row, TableDelta, TableRowCount,
};
use crate::schema::SchemaPlan;

#[derive(Debug)]
pub struct Model {
    schema: SchemaPlan,
    committed_tables: Vec<TableState>,
    pending_tx: Option<PendingTx>,
}

#[derive(Debug)]
struct TableState {
    rows: Vec<Row>,
}

#[derive(Debug)]
struct PendingTx {
    tables: Vec<PendingTable>,
}

#[derive(Debug, Default)]
struct PendingTable {
    inserts: Vec<Row>,
    deletes: Vec<Row>,
}

impl PendingTable {
    fn is_touched(&self) -> bool {
        !self.inserts.is_empty() || !self.deletes.is_empty()
    }

    fn is_deleted(&self, row: &Row) -> bool {
        self.deletes.iter().any(|deleted| deleted == row)
    }

    fn after_contains(&self, before_rows: &[Row], row: &Row) -> bool {
        self.inserts.iter().any(|inserted| inserted == row)
            || before_rows
                .iter()
                .any(|before| !self.is_deleted(before) && before == row)
    }
}

impl PendingTx {
    fn new(table_count: usize) -> Self {
        Self {
            tables: (0..table_count).map(|_| PendingTable::default()).collect(),
        }
    }
}

impl Model {
    pub fn new(schema: SchemaPlan) -> Self {
        let committed_tables = schema.tables.iter().map(|_| TableState { rows: vec![] }).collect();
        Self {
            schema,
            committed_tables,
            pending_tx: None,
        }
    }

    pub fn schema(&self) -> &SchemaPlan {
        &self.schema
    }

    fn pending_table(&self, table: usize) -> Option<&PendingTable> {
        self.pending_tx.as_ref().map(|pending_tx| &pending_tx.tables[table])
    }

    fn pending_table_mut(&mut self, table: usize) -> &mut PendingTable {
        debug_assert!(self.pending_tx.is_some());
        &mut self.pending_tx.as_mut().expect("active transaction").tables[table]
    }

    fn committed_row_is_visible(&self, table: usize, row: &Row) -> bool {
        self.pending_table(table)
            .is_none_or(|pending_table| !pending_table.is_deleted(row))
    }

    fn visible_count(&self, table: usize) -> u64 {
        let committed_count = self.committed_tables[table]
            .rows
            .iter()
            .filter(|row| self.committed_row_is_visible(table, row))
            .count();
        let pending_insert_count = self
            .pending_table(table)
            .map_or(0, |pending_table| pending_table.inserts.len());
        (committed_count + pending_insert_count) as u64
    }

    fn any_visible_row(&self, table: usize, mut matches: impl FnMut(&Row) -> bool) -> bool {
        for row in &self.committed_tables[table].rows {
            if self.committed_row_is_visible(table, row) && matches(row) {
                return true;
            }
        }

        if let Some(pending_table) = self.pending_table(table) {
            for row in &pending_table.inserts {
                if matches(row) {
                    return true;
                }
            }
        }

        false
    }

    fn visible_contains(&self, table: usize, row: &Row) -> bool {
        self.any_visible_row(table, |visible_row| visible_row == row)
    }

    fn committed_visible_contains(&self, table: usize, row: &Row) -> bool {
        self.committed_tables[table]
            .rows
            .iter()
            .any(|committed_row| self.committed_row_is_visible(table, committed_row) && committed_row == row)
    }

    fn committed_visible_pk_match(&self, table: usize, pk_col: usize, row: &Row) -> Option<Row> {
        self.committed_tables[table]
            .rows
            .iter()
            .find(|committed_row| {
                self.committed_row_is_visible(table, committed_row)
                    && committed_row.elements[pk_col] == row.elements[pk_col]
            })
            .cloned()
    }

    fn violates_unique_constraint(&self, table: usize, row: &Row) -> bool {
        let table_plan = &self.schema.tables[table];
        for constraint in &table_plan.unique_constraints {
            if self.any_visible_row(table, |visible_row| {
                constraint
                    .columns
                    .iter()
                    .all(|&col| visible_row.elements[col] == row.elements[col])
            }) {
                return true;
            }
        }
        false
    }

    pub fn apply(&mut self, interaction: &Interaction) -> Observation {
        match interaction {
            Interaction::BeginMutTx => {
                debug_assert!(self.pending_tx.is_none());
                self.pending_tx = Some(PendingTx::new(self.committed_tables.len()));
                Observation::BeganMutTx
            }
            Interaction::Insert { table, row } => {
                debug_assert!(self.pending_tx.is_some());
                let primary_key = self.schema.tables[*table].primary_key;
                let count_before = self.visible_count(*table);

                if self.violates_unique_constraint(*table, row) || self.visible_contains(*table, row) {
                    return Observation::Inserted {
                        rows_count: count_before,
                    };
                }

                if let Some(pk_col) = primary_key {
                    if let Some(replaced_row) = self.committed_visible_pk_match(*table, pk_col, row) {
                        let pending_table = self.pending_table_mut(*table);
                        if !pending_table.is_deleted(&replaced_row) {
                            pending_table.deletes.push(replaced_row);
                        }
                        pending_table.inserts.push(row.clone());
                        return Observation::Inserted {
                            rows_count: count_before,
                        };
                    }

                    let pending_table = self.pending_table_mut(*table);
                    if let Some(pos) = pending_table
                        .inserts
                        .iter()
                        .position(|inserted| inserted.elements[pk_col] == row.elements[pk_col])
                    {
                        pending_table.inserts[pos] = row.clone();
                        return Observation::Inserted {
                            rows_count: count_before,
                        };
                    }
                }

                self.pending_table_mut(*table).inserts.push(row.clone());
                Observation::Inserted {
                    rows_count: count_before + 1,
                }
            }
            Interaction::Delete { table, row } => {
                debug_assert!(self.pending_tx.is_some());
                if self.visible_contains(*table, row) {
                    let committed_has_row = self.committed_visible_contains(*table, row);
                    let pending_table = self.pending_table_mut(*table);
                    pending_table.inserts.retain(|inserted| inserted != row);
                    if committed_has_row && !pending_table.is_deleted(row) {
                        pending_table.deletes.push(row.clone());
                    }
                }
                Observation::Deleted {
                    rows_count: self.visible_count(*table),
                }
            }
            Interaction::CommitTx => {
                debug_assert!(self.pending_tx.is_some());
                let pending_tx = self.pending_tx.take().expect("active transaction");
                let delta = self.commit_pending(pending_tx);
                Observation::Committed { delta }
            }
            Interaction::Replay => {
                self.pending_tx = None;
                Observation::Replayed {
                    state: self.light_snapshot(),
                }
            }
        }
    }

    fn commit_pending(&mut self, pending_tx: PendingTx) -> CommitDelta {
        let mut tables = Vec::new();

        for (table, pending_table) in pending_tx.tables.into_iter().enumerate() {
            if !pending_table.is_touched() {
                continue;
            }

            let before_rows = &self.committed_tables[table].rows;
            let inserts = normalize_rows(
                pending_table
                    .inserts
                    .iter()
                    .filter(|inserted| !before_rows.contains(inserted))
                    .cloned()
                    .collect(),
            );
            let deletes = normalize_rows(
                before_rows
                    .iter()
                    .filter(|before| !pending_table.after_contains(before_rows, before))
                    .cloned()
                    .collect(),
            );
            let after_count = before_rows
                .iter()
                .filter(|before| !pending_table.is_deleted(before))
                .count()
                + pending_table.inserts.len();
            let truncated = !before_rows.is_empty() && after_count == 0 && !deletes.is_empty();

            if !inserts.is_empty() || !deletes.is_empty() || truncated {
                tables.push(TableDelta {
                    table,
                    inserts,
                    deletes,
                    truncated,
                });
            }

            let committed_rows = &mut self.committed_tables[table].rows;
            committed_rows.retain(|row| !pending_table.is_deleted(row));
            committed_rows.extend(pending_table.inserts);
        }

        CommitDelta { tables }
    }

    pub fn in_mut_tx(&self) -> bool {
        self.pending_tx.is_some()
    }

    pub fn row_count(&self, table: usize) -> usize {
        self.visible_count(table) as usize
    }

    pub fn row(&self, table: usize, row: usize) -> Option<&Row> {
        let mut remaining = row;
        for committed_row in &self.committed_tables[table].rows {
            if !self.committed_row_is_visible(table, committed_row) {
                continue;
            }
            if remaining == 0 {
                return Some(committed_row);
            }
            remaining -= 1;
        }

        self.pending_table(table)
            .and_then(|pending_table| pending_table.inserts.get(remaining))
    }

    #[cfg(test)]
    pub fn rows(&self, table: usize) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.row_count(table));
        for committed_row in &self.committed_tables[table].rows {
            if self.committed_row_is_visible(table, committed_row) {
                rows.push(committed_row.clone());
            }
        }
        if let Some(pending_table) = self.pending_table(table) {
            rows.extend(pending_table.inserts.iter().cloned());
        }
        rows
    }

    fn light_snapshot(&self) -> CountState {
        let row_counts = (0..self.schema.tables.len())
            .map(|table| TableRowCount {
                table,
                count: self.visible_count(table),
            })
            .collect();
        CountState { row_counts }
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::AlgebraicValue;

    use super::*;
    use crate::schema::{ColumnPlan, IndexAlgorithm, IndexPlan, TablePlan, Type, UniqueConstraintPlan};

    fn schema() -> SchemaPlan {
        SchemaPlan {
            tables: vec![TablePlan {
                name: "items".into(),
                columns: vec![ColumnPlan {
                    name: "id".into(),
                    ty: Type::U64,
                }],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![0],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: vec![],
                is_public: true,
            }],
        }
    }

    fn row(id: u64) -> Row {
        Row {
            elements: vec![AlgebraicValue::U64(id)].into(),
        }
    }

    #[test]
    fn begin_mut_tx_does_not_clone_committed_tables() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));

        model.apply(&Interaction::BeginMutTx);

        let pending_tx = model.pending_tx.as_ref().expect("active transaction");
        assert!(pending_tx.tables.iter().all(|table| !table.is_touched()));
        assert_eq!(model.rows(0), vec![row(1)]);
    }

    #[test]
    fn insert_records_delta_without_cloning_committed_rows() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));

        model.apply(&Interaction::BeginMutTx);
        model.apply(&Interaction::Insert { table: 0, row: row(2) });

        let pending_table = &model.pending_tx.as_ref().expect("active transaction").tables[0];
        assert_eq!(pending_table.inserts, vec![row(2)]);
        assert!(pending_table.deletes.is_empty());
        assert_eq!(model.committed_tables[0].rows, vec![row(1)]);
        assert_eq!(model.rows(0), vec![row(1), row(2)]);
    }

    #[test]
    fn delete_records_marker_without_cloning_committed_rows() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));
        model.committed_tables[0].rows.push(row(2));

        model.apply(&Interaction::BeginMutTx);
        model.apply(&Interaction::Delete { table: 0, row: row(1) });

        let pending_table = &model.pending_tx.as_ref().expect("active transaction").tables[0];
        assert!(pending_table.inserts.is_empty());
        assert_eq!(pending_table.deletes, vec![row(1)]);
        assert_eq!(model.committed_tables[0].rows, vec![row(1), row(2)]);
        assert_eq!(model.rows(0), vec![row(2)]);
    }

    #[test]
    fn insert_is_visible_before_commit_and_replay_rolls_back() {
        let mut model = Model::new(schema());

        model.apply(&Interaction::BeginMutTx);
        model.apply(&Interaction::Insert { table: 0, row: row(1) });
        assert_eq!(model.row_count(0), 1);

        model.apply(&Interaction::Replay);
        model.apply(&Interaction::BeginMutTx);
        assert_eq!(model.row_count(0), 0);
    }

    #[test]
    fn commit_applies_only_pending_overlay() {
        let mut model = Model::new(schema());

        model.apply(&Interaction::BeginMutTx);
        model.apply(&Interaction::Insert { table: 0, row: row(1) });
        let observation = model.apply(&Interaction::CommitTx);

        let Observation::Committed { delta, .. } = observation else {
            panic!("expected commit observation");
        };
        assert_eq!(delta.tables.len(), 1);
        assert_eq!(delta.tables[0].inserts, vec![row(1)]);
        assert_eq!(model.committed_tables[0].rows, vec![row(1)]);
    }

    #[test]
    fn delete_is_visible_before_commit() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));

        model.apply(&Interaction::BeginMutTx);
        model.apply(&Interaction::Delete { table: 0, row: row(1) });

        assert_eq!(model.row_count(0), 0);
    }
}
