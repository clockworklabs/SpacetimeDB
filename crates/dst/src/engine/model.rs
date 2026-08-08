use spacetimedb_lib::{AlgebraicValue, ProductValue};

use super::migrations::MigrationExpectation;
use super::row::{normalize_rows, Row};
use super::state::{schema_state_for_plan, CommitDelta, CountState, TableDelta, TableRowCount, TableRows};
use super::workload::{InsertOutcome, Interaction, Observation};
use crate::schema::{ColumnPlan, SchemaPlan, TablePlan};

#[derive(Debug)]
pub struct Model {
    schema: SchemaPlan,
    committed_tables: Vec<TableState>,
    pending_tx: Option<PendingTx>,
}

#[derive(Debug)]
struct TableState {
    rows: Vec<Row>,
    ever_inserted: bool,
}

#[derive(Debug)]
struct PendingTx {
    tables: Vec<PendingTable>,
}

// Keep mutable transactions as an overlay: committed rows stay shared, while
// pending tables record only new rows and delete markers.
#[derive(Debug, Default)]
struct PendingTable {
    inserts: Vec<Row>,
    deletes: Vec<Row>,
}

impl PendingTable {
    fn is_deleted(&self, row: &Row) -> bool {
        self.deletes.iter().any(|deleted| deleted == row)
    }

    fn insert(&mut self, row: Row) {
        if let Some(index) = self.deletes.iter().position(|deleted| deleted == &row) {
            self.deletes.remove(index);
        } else {
            self.inserts.push(row);
        }
    }

    fn delete(&mut self, row: &Row, committed_has_row: bool) {
        self.inserts.retain(|inserted| inserted != row);
        if committed_has_row && !self.is_deleted(row) {
            self.deletes.push(row.clone());
        }
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
        let committed_tables = schema
            .tables
            .iter()
            .map(|_| TableState {
                rows: vec![],
                ever_inserted: false,
            })
            .collect();
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

    fn visible_committed_rows(&self, table: usize) -> impl Iterator<Item = &Row> + '_ {
        let pending_table = self.pending_table(table);
        self.committed_tables[table]
            .rows
            .iter()
            .filter(move |row| pending_table.is_none_or(|pending_table| !pending_table.is_deleted(row)))
    }

    // Visibility is committed rows minus delete markers, followed by pending inserts.
    fn visible_rows(&self, table: usize) -> impl Iterator<Item = &Row> + '_ {
        self.visible_committed_rows(table).chain(
            self.pending_table(table)
                .into_iter()
                .flat_map(|pending_table| pending_table.inserts.iter()),
        )
    }

    fn visible_count(&self, table: usize) -> u64 {
        self.visible_rows(table).count() as u64
    }

    fn any_visible_row(&self, table: usize, matches: impl FnMut(&Row) -> bool) -> bool {
        self.visible_rows(table).any(matches)
    }

    fn violates_unique_constraint(&self, table: usize, row: &Row, ignore_sequence_constraints: bool) -> bool {
        let table_plan = &self.schema.tables[table];
        for constraint in &table_plan.unique_constraints {
            if ignore_sequence_constraints
                && constraint
                    .columns
                    .iter()
                    .any(|&column| column_has_sequence(table_plan, column))
            {
                continue;
            }

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

    pub(crate) fn insert_would_violate_unique_constraint(&self, table: usize, row: &Row) -> bool {
        !self.any_visible_row(table, |visible_row| visible_row == row)
            && self.violates_unique_constraint(table, row, true)
    }

    pub fn apply(&mut self, interaction: &Interaction, observation: &Observation) -> anyhow::Result<Observation> {
        match interaction {
            Interaction::BeginMutTx => {
                anyhow::ensure!(
                    matches!(observation, Observation::BeganMutTx),
                    "begin-mut-tx produced unexpected observation"
                );
                debug_assert!(self.pending_tx.is_none());
                self.pending_tx = Some(PendingTx::new(self.schema.tables.len()));
                Ok(Observation::BeganMutTx)
            }
            Interaction::Insert { table, .. } => {
                debug_assert!(self.pending_tx.is_some());
                let Observation::Inserted { outcome } = observation else {
                    anyhow::bail!("insert produced unexpected observation");
                };

                self.committed_tables[*table].ever_inserted = true;
                match outcome {
                    InsertOutcome::Accepted(row) => {
                        let already_visible = self.any_visible_row(*table, |visible_row| visible_row == row);
                        anyhow::ensure!(
                            already_visible || !self.violates_unique_constraint(*table, row, false),
                            "target accepted row that violates a visible unique constraint"
                        );
                        if !already_visible {
                            self.pending_table_mut(*table).insert(row.clone());
                        }
                        Ok(Observation::Inserted {
                            outcome: InsertOutcome::Accepted(row.clone()),
                        })
                    }
                    InsertOutcome::UniqueConstraintViolation { .. } => Ok(observation.clone()),
                }
            }
            Interaction::Delete { table, row } => {
                anyhow::ensure!(
                    matches!(observation, Observation::Deleted),
                    "delete produced unexpected observation"
                );
                debug_assert!(self.pending_tx.is_some());
                if self.any_visible_row(*table, |visible_row| visible_row == row) {
                    let committed_has_row = self.visible_committed_rows(*table).any(|committed| committed == row);
                    self.pending_table_mut(*table).delete(row, committed_has_row);
                }
                Ok(Observation::Deleted)
            }
            Interaction::CommitTx => {
                anyhow::ensure!(
                    matches!(observation, Observation::Committed { .. }),
                    "commit produced unexpected observation"
                );
                debug_assert!(self.pending_tx.is_some());
                let pending_tx = self.pending_tx.take().expect("active transaction");
                let delta = self.commit_pending(pending_tx);
                Ok(Observation::Committed { delta })
            }
            Interaction::Migrate(migration) => {
                debug_assert!(self.pending_tx.is_none());
                match migration.expectation() {
                    MigrationExpectation::Accepted => {
                        let old_schema = std::mem::replace(&mut self.schema, migration.schema().clone());
                        let old_tables = std::mem::take(&mut self.committed_tables);
                        self.committed_tables = remap_table_states(&old_schema, &self.schema, old_tables);
                        Ok(Observation::Migrated)
                    }
                    MigrationExpectation::Rejected => Ok(Observation::MigrationRejected),
                }
            }
            Interaction::Replay => {
                anyhow::ensure!(
                    matches!(observation, Observation::Replayed { .. }),
                    "replay produced unexpected observation"
                );
                self.pending_tx = None;
                Ok(Observation::Replayed {
                    state: self.count_state(),
                })
            }
        }
    }

    fn commit_pending(&mut self, pending_tx: PendingTx) -> CommitDelta {
        let PendingTx { tables: pending_tables } = pending_tx;
        let mut tables = Vec::new();

        for (table, pending_table) in pending_tables.into_iter().enumerate() {
            if pending_table.inserts.is_empty() && pending_table.deletes.is_empty() {
                continue;
            }

            let before_rows = &self.committed_tables[table].rows;
            let inserts = normalize_rows(pending_table.inserts.clone());
            let deletes = normalize_rows(pending_table.deletes.clone());
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

    pub(crate) fn pending_has_effective_writes(&self) -> bool {
        self.pending_tx.as_ref().is_some_and(|pending_tx| {
            pending_tx
                .tables
                .iter()
                .any(|table| !table.inserts.is_empty() || !table.deletes.is_empty())
        })
    }

    pub fn row_count(&self, table: usize) -> usize {
        self.visible_count(table) as usize
    }

    pub fn ever_inserted(&self, table: usize) -> bool {
        self.committed_tables[table].ever_inserted
    }

    pub(crate) fn row_count_by_table_name(&self, table: &str) -> usize {
        self.table_index(table).map_or(0, |table| self.row_count(table))
    }

    pub(crate) fn ever_inserted_by_table_name(&self, table: &str) -> bool {
        self.table_index(table)
            .is_some_and(|table| self.committed_tables[table].ever_inserted)
    }

    fn table_index(&self, table: &str) -> Option<usize> {
        self.schema
            .tables
            .iter()
            .position(|table_plan| table_plan.name == table)
    }

    pub fn row(&self, table: usize, row: usize) -> Option<&Row> {
        self.visible_rows(table).nth(row)
    }

    #[cfg(test)]
    pub fn rows(&self, table: usize) -> Vec<Row> {
        self.visible_rows(table).cloned().collect()
    }

    fn count_state(&self) -> CountState {
        let row_counts = (0..self.schema.tables.len())
            .map(|table| TableRowCount {
                table,
                count: self.visible_count(table),
            })
            .collect();
        let table_rows = (0..self.schema.tables.len())
            .map(|table| TableRows {
                table,
                rows: normalize_rows(self.visible_rows(table).cloned().collect()),
            })
            .collect();

        CountState {
            row_counts,
            table_rows,
            schema: schema_state_for_plan(&self.schema),
        }
    }
}

fn column_has_sequence(table: &TablePlan, column: usize) -> bool {
    table.sequences.iter().any(|sequence| sequence.column == column)
}

fn remap_table_states(
    old_schema: &SchemaPlan,
    new_schema: &SchemaPlan,
    old_tables: Vec<TableState>,
) -> Vec<TableState> {
    let mut old_tables = old_tables.into_iter().map(Some).collect::<Vec<_>>();
    new_schema
        .tables
        .iter()
        .map(|new_table| {
            let Some(old_table_idx) = old_schema
                .tables
                .iter()
                .position(|old_table| old_table.name == new_table.name)
            else {
                return TableState {
                    rows: vec![],
                    ever_inserted: false,
                };
            };

            let old_table = &old_schema.tables[old_table_idx];
            let old_state = old_tables[old_table_idx]
                .take()
                .expect("old table state is consumed once");
            remap_table_state(old_table, new_table, old_state)
        })
        .collect()
}

fn remap_table_state(old_table: &TablePlan, new_table: &TablePlan, state: TableState) -> TableState {
    TableState {
        rows: state
            .rows
            .into_iter()
            .map(|row| remap_row(old_table, new_table, row))
            .collect(),
        ever_inserted: state.ever_inserted,
    }
}

fn remap_row(old_table: &TablePlan, new_table: &TablePlan, row: Row) -> Row {
    let elements = new_table
        .columns
        .iter()
        .map(|new_column| remap_value(old_table, new_column, &row))
        .collect::<Vec<_>>();
    ProductValue {
        elements: elements.into_boxed_slice(),
    }
}

fn remap_value(old_table: &TablePlan, new_column: &ColumnPlan, row: &Row) -> AlgebraicValue {
    old_table
        .columns
        .iter()
        .position(|old_column| old_column.name == new_column.name)
        .map(|old_column| row.elements[old_column].clone())
        .unwrap_or_else(|| new_column.ty.default_value())
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::AlgebraicValue;

    use super::*;
    use crate::schema::{ColumnPlan, IndexAlgorithm, IndexPlan, SequencePlan, TablePlan, Type, UniqueConstraintPlan};

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
                is_event: false,
            }],
        }
    }

    fn row(id: u64) -> Row {
        Row {
            elements: vec![AlgebraicValue::U64(id)].into(),
        }
    }

    fn observe(model: &mut Model, interaction: Interaction, observation: Observation) -> Observation {
        model
            .apply(&interaction, &observation)
            .expect("model should accept target observation")
    }

    fn accepted(row: Row) -> Observation {
        Observation::Inserted {
            outcome: InsertOutcome::Accepted(row),
        }
    }

    fn committed() -> Observation {
        Observation::Committed {
            delta: CommitDelta { tables: Vec::new() },
        }
    }

    fn replayed(model: &Model) -> Observation {
        Observation::Replayed {
            state: model.count_state(),
        }
    }

    fn keyed_payload_schema(sequence: bool) -> SchemaPlan {
        SchemaPlan {
            tables: vec![TablePlan {
                name: "items".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: Type::U64,
                    },
                    ColumnPlan {
                        name: "payload".into(),
                        ty: Type::String,
                    },
                ],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![0],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: sequence
                    .then(|| SequencePlan::new(0, Type::U64).expect("u64 sequence"))
                    .into_iter()
                    .collect(),
                is_public: true,
                is_event: false,
            }],
        }
    }

    fn payload_row(id: u64, payload: &str) -> Row {
        Row {
            elements: vec![AlgebraicValue::U64(id), AlgebraicValue::String(payload.into())].into(),
        }
    }

    #[test]
    fn insert_would_violate_unique_constraint_distinguishes_duplicates_from_conflicts() {
        let mut model = Model::new(keyed_payload_schema(false));
        model.committed_tables[0].rows.push(payload_row(1, "a"));

        assert!(!model.insert_would_violate_unique_constraint(0, &payload_row(1, "a")));
        assert!(model.insert_would_violate_unique_constraint(0, &payload_row(1, "b")));
        assert!(!model.insert_would_violate_unique_constraint(0, &payload_row(2, "b")));
    }

    #[test]
    fn insert_would_violate_unique_constraint_treats_sequence_constraints_as_ambiguous() {
        let mut model = Model::new(keyed_payload_schema(true));
        model.committed_tables[0].rows.push(payload_row(1, "a"));

        assert!(!model.insert_would_violate_unique_constraint(0, &payload_row(0, "b")));
    }

    #[test]
    fn begin_mut_tx_does_not_clone_committed_tables() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));

        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);

        let pending_tx = model.pending_tx.as_ref().expect("active transaction");
        assert!(pending_tx
            .tables
            .iter()
            .all(|table| table.inserts.is_empty() && table.deletes.is_empty()));
        assert_eq!(model.rows(0), vec![row(1)]);
    }

    #[test]
    fn insert_records_delta_without_cloning_committed_rows() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));

        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);
        observe(
            &mut model,
            Interaction::Insert { table: 0, row: row(2) },
            accepted(row(2)),
        );

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

        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);
        observe(
            &mut model,
            Interaction::Delete { table: 0, row: row(1) },
            Observation::Deleted,
        );

        let pending_table = &model.pending_tx.as_ref().expect("active transaction").tables[0];
        assert!(pending_table.inserts.is_empty());
        assert_eq!(pending_table.deletes, vec![row(1)]);
        assert_eq!(model.committed_tables[0].rows, vec![row(1), row(2)]);
        assert_eq!(model.rows(0), vec![row(2)]);
    }

    #[test]
    fn insert_is_visible_before_commit_and_replay_rolls_back() {
        let mut model = Model::new(schema());

        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);
        observe(
            &mut model,
            Interaction::Insert { table: 0, row: row(1) },
            accepted(row(1)),
        );
        assert_eq!(model.row_count(0), 1);

        let replay = replayed(&model);
        observe(&mut model, Interaction::Replay, replay);
        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);
        assert_eq!(model.row_count(0), 0);
    }

    #[test]
    fn commit_applies_only_pending_overlay() {
        let mut model = Model::new(schema());

        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);
        observe(
            &mut model,
            Interaction::Insert { table: 0, row: row(1) },
            accepted(row(1)),
        );
        let observation = observe(&mut model, Interaction::CommitTx, committed());

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

        observe(&mut model, Interaction::BeginMutTx, Observation::BeganMutTx);
        observe(
            &mut model,
            Interaction::Delete { table: 0, row: row(1) },
            Observation::Deleted,
        );

        assert_eq!(model.row_count(0), 0);
    }
}
