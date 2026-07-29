use spacetimedb_lib::{db::raw_def::SEQUENCE_ALLOCATION_STEP, AlgebraicValue, ProductValue};

use super::migrations::MigrationExpectation;
use super::row::{normalize_rows, Row};
use super::state::{schema_state_for_plan, CommitDelta, CountState, TableDelta, TableRowCount, TableRows};
use super::workload::{InsertOutcome, Interaction, Observation};
use crate::schema::{ColumnPlan, SchemaPlan, SequencePlan, TablePlan, Type};

#[derive(Debug)]
pub struct Model {
    schema: SchemaPlan,
    committed_tables: Vec<TableState>,
    sequences: Vec<Vec<ModelSequence>>,
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
    sequence_allocations: Vec<Vec<Option<i128>>>,
}

#[derive(Debug, Clone)]
struct ModelSequence {
    column: usize,
    ty: Type,
    value: i128,
    allocated: i128,
    durable_allocated: i128,
    min_value: i128,
    max_value: i128,
    increment: i128,
}

impl ModelSequence {
    fn new(plan: &SequencePlan, ty: Type) -> Self {
        let start = plan.start.unwrap_or(1);
        Self {
            column: plan.column,
            ty,
            value: start,
            allocated: start,
            durable_allocated: start,
            min_value: plan.min_value.unwrap_or(1),
            max_value: plan.max_value.unwrap_or(i128::MAX),
            increment: plan.increment,
        }
    }

    fn same_definition(&self, plan: &SequencePlan, ty: Type) -> bool {
        self.ty == ty
            && self.min_value == plan.min_value.unwrap_or(1)
            && self.max_value == plan.max_value.unwrap_or(i128::MAX)
            && self.increment == plan.increment
    }

    fn with_column(mut self, column: usize) -> Self {
        self.column = column;
        self
    }

    fn generate(&mut self) -> (AlgebraicValue, Option<i128>) {
        let old_allocated = self.allocated;
        if self.needs_allocation() {
            self.allocate_steps(SEQUENCE_ALLOCATION_STEP as usize);
        }

        let value = self.value;
        self.value = self.next_value();
        let allocation = (self.allocated != old_allocated).then_some(self.allocated);
        (self.value_to_algebraic(value), allocation)
    }

    fn reset_to_durable(&mut self) {
        self.value = self.durable_allocated;
        self.allocated = self.durable_allocated;
    }

    fn needs_allocation(&self) -> bool {
        self.value == self.allocated
    }

    fn allocate_steps(&mut self, steps: usize) {
        if !self.needs_allocation() {
            return;
        }

        let original_allocation = self.allocated;
        for _ in 0..steps {
            let next = next_sequence_value(self.min_value, self.max_value, self.increment, self.allocated);
            if next == original_allocation {
                break;
            }
            self.allocated = next;
        }
        debug_assert!(!self.needs_allocation(), "sequence allocation should make progress");
    }

    fn next_value(&self) -> i128 {
        next_sequence_value(self.min_value, self.max_value, self.increment, self.value)
    }

    fn value_to_algebraic(&self, value: i128) -> AlgebraicValue {
        match self.ty {
            Type::I64 => AlgebraicValue::I64(value as i64),
            Type::U64 => AlgebraicValue::U64(value as u64),
            _ => unreachable!("sequence columns are integral in the DST schema"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ColumnDomain {
    pub(crate) values: Vec<AlgebraicValue>,
    pub(crate) single_column_unique: bool,
    pub(crate) single_column_indexed: bool,
    pub(crate) sequenced: bool,
}

impl ColumnDomain {
    pub(crate) fn integral_values(&self) -> impl Iterator<Item = i128> + '_ {
        self.values.iter().filter_map(|value| match value {
            AlgebraicValue::U64(value) => Some(*value as i128),
            AlgebraicValue::I64(value) => Some(*value as i128),
            _ => None,
        })
    }
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
}

impl PendingTx {
    fn new(sequences: &[Vec<ModelSequence>]) -> Self {
        Self {
            tables: (0..sequences.len()).map(|_| PendingTable::default()).collect(),
            sequence_allocations: sequences
                .iter()
                .map(|table_sequences| vec![None; table_sequences.len()])
                .collect(),
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
        let sequences = sequence_states_for_schema(&schema);
        Self {
            schema,
            committed_tables,
            sequences,
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

    fn pending_sequence_allocation_mut(&mut self, table: usize, sequence: usize) -> &mut Option<i128> {
        debug_assert!(self.pending_tx.is_some());
        &mut self
            .pending_tx
            .as_mut()
            .expect("active transaction")
            .sequence_allocations[table][sequence]
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

    pub(crate) fn insert_would_violate_unique_constraint(&self, table: usize, row: &Row) -> bool {
        let row = self.project_sequence_values(table, row);
        !self.any_visible_row(table, |visible_row| visible_row == &row) && self.violates_unique_constraint(table, &row)
    }

    fn project_sequence_values(&self, table: usize, row: &Row) -> Row {
        let mut row = row.clone();
        let mut sequences = self.sequences[table].clone();

        for sequence in &mut sequences {
            let column = sequence.column;
            if row.elements[column].is_numeric_zero() {
                let (value, _allocation) = sequence.generate();
                row.elements[column] = value;
            }
        }

        row
    }

    fn apply_sequence_values(&mut self, table: usize, row: &Row) -> Row {
        let mut row = row.clone();
        let sequence_count = self.sequences[table].len();

        for sequence_idx in 0..sequence_count {
            let column = self.sequences[table][sequence_idx].column;
            if !row.elements[column].is_numeric_zero() {
                continue;
            }

            let (value, allocation) = self.sequences[table][sequence_idx].generate();
            row.elements[column] = value;
            if let Some(allocation) = allocation {
                *self.pending_sequence_allocation_mut(table, sequence_idx) = Some(allocation);
            }
        }

        row
    }

    pub fn apply(&mut self, interaction: &Interaction) -> Observation {
        match interaction {
            Interaction::BeginMutTx => {
                debug_assert!(self.pending_tx.is_none());
                self.pending_tx = Some(PendingTx::new(&self.sequences));
                Observation::BeganMutTx
            }
            Interaction::Insert { table, row } => {
                debug_assert!(self.pending_tx.is_some());
                self.committed_tables[*table].ever_inserted = true;
                let row = self.apply_sequence_values(*table, row);

                if self.any_visible_row(*table, |visible_row| visible_row == &row) {
                    return Observation::Inserted {
                        outcome: InsertOutcome::Accepted(row),
                    };
                }

                if self.violates_unique_constraint(*table, &row) {
                    return Observation::Inserted {
                        outcome: InsertOutcome::UniqueConstraintViolation {
                            details: "model unique constraint".into(),
                        },
                    };
                }

                self.pending_table_mut(*table).inserts.push(row.clone());
                Observation::Inserted {
                    outcome: InsertOutcome::Accepted(row),
                }
            }
            Interaction::Delete { table, row } => {
                debug_assert!(self.pending_tx.is_some());
                if self.any_visible_row(*table, |visible_row| visible_row == row) {
                    let committed_has_row = self.visible_committed_rows(*table).any(|committed| committed == row);
                    let pending_table = self.pending_table_mut(*table);
                    pending_table.inserts.retain(|inserted| inserted != row);
                    if committed_has_row && !pending_table.is_deleted(row) {
                        pending_table.deletes.push(row.clone());
                    }
                }
                Observation::Deleted
            }
            Interaction::CommitTx => {
                debug_assert!(self.pending_tx.is_some());
                let pending_tx = self.pending_tx.take().expect("active transaction");
                let delta = self.commit_pending(pending_tx);
                Observation::Committed { delta }
            }
            Interaction::Migrate(migration) => {
                debug_assert!(self.pending_tx.is_none());
                match migration.expectation() {
                    MigrationExpectation::Accepted => {
                        let old_schema = std::mem::replace(&mut self.schema, migration.schema().clone());
                        let old_tables = std::mem::take(&mut self.committed_tables);
                        let old_sequences = std::mem::take(&mut self.sequences);
                        self.committed_tables = remap_table_states(&old_schema, &self.schema, old_tables);
                        self.sequences = remap_sequence_states(&old_schema, &self.schema, old_sequences);
                        Observation::Migrated
                    }
                    MigrationExpectation::Rejected => Observation::MigrationRejected,
                }
            }
            Interaction::Replay => {
                self.pending_tx = None;
                self.reset_sequences_to_durable();
                Observation::Replayed {
                    state: self.count_state(),
                }
            }
        }
    }

    fn commit_pending(&mut self, pending_tx: PendingTx) -> CommitDelta {
        let PendingTx {
            tables: pending_tables,
            sequence_allocations,
        } = pending_tx;
        let mut tables = Vec::new();

        for (table, pending_table) in pending_tables.into_iter().enumerate() {
            if pending_table.inserts.is_empty() && pending_table.deletes.is_empty() {
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
            // A delete followed by the same insert leaves the committed set unchanged.
            let deletes = normalize_rows(
                before_rows
                    .iter()
                    .filter(|before| pending_table.is_deleted(before) && !pending_table.inserts.contains(before))
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

        for (table, allocations) in sequence_allocations.into_iter().enumerate() {
            for (sequence, allocation) in allocations.into_iter().enumerate() {
                if let Some(allocation) = allocation {
                    self.sequences[table][sequence].durable_allocated = allocation;
                }
            }
        }

        CommitDelta { tables }
    }

    fn reset_sequences_to_durable(&mut self) {
        for sequence in self.sequences.iter_mut().flatten() {
            sequence.reset_to_durable();
        }
    }

    pub fn in_mut_tx(&self) -> bool {
        self.pending_tx.is_some()
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

    pub(crate) fn column_domain_by_name(&self, table: &str, column: &str) -> Option<ColumnDomain> {
        let table = self.table_index(table)?;
        let column = self.schema.tables[table]
            .columns
            .iter()
            .position(|column_plan| column_plan.name == column)?;
        Some(self.column_domain(table, column))
    }

    fn table_index(&self, table: &str) -> Option<usize> {
        self.schema
            .tables
            .iter()
            .position(|table_plan| table_plan.name == table)
    }

    pub(crate) fn column_domain(&self, table: usize, column: usize) -> ColumnDomain {
        let table_plan = &self.schema.tables[table];
        ColumnDomain {
            values: self
                .visible_rows(table)
                .map(|row| row.elements[column].clone())
                .collect(),
            single_column_unique: table_plan
                .unique_constraints
                .iter()
                .any(|constraint| constraint.columns == [column]),
            single_column_indexed: table_plan.indexes.iter().any(|index| index.columns == [column]),
            sequenced: table_plan.sequences.iter().any(|sequence| sequence.column == column),
        }
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

fn sequence_states_for_schema(schema: &SchemaPlan) -> Vec<Vec<ModelSequence>> {
    schema.tables.iter().map(sequence_states_for_table).collect()
}

fn sequence_states_for_table(table: &TablePlan) -> Vec<ModelSequence> {
    table
        .sequences
        .iter()
        .map(|sequence| ModelSequence::new(sequence, table.columns[sequence.column].ty))
        .collect()
}

fn remap_sequence_states(
    old_schema: &SchemaPlan,
    new_schema: &SchemaPlan,
    old_sequences: Vec<Vec<ModelSequence>>,
) -> Vec<Vec<ModelSequence>> {
    new_schema
        .tables
        .iter()
        .map(|new_table| {
            let Some(old_table_idx) = old_schema
                .tables
                .iter()
                .position(|old_table| old_table.name == new_table.name)
            else {
                return sequence_states_for_table(new_table);
            };

            let old_table = &old_schema.tables[old_table_idx];
            new_table
                .sequences
                .iter()
                .map(|new_sequence| {
                    remap_sequence_state(old_table, new_table, &old_sequences[old_table_idx], new_sequence)
                })
                .collect()
        })
        .collect()
}

fn remap_sequence_state(
    old_table: &TablePlan,
    new_table: &TablePlan,
    old_sequences: &[ModelSequence],
    new_sequence: &SequencePlan,
) -> ModelSequence {
    let new_column = &new_table.columns[new_sequence.column];
    let Some(old_column_idx) = old_table
        .columns
        .iter()
        .position(|old_column| old_column.name == new_column.name)
    else {
        return ModelSequence::new(new_sequence, new_column.ty);
    };

    let Some(old_sequence_idx) = old_table
        .sequences
        .iter()
        .position(|old_sequence| old_sequence.column == old_column_idx)
    else {
        return ModelSequence::new(new_sequence, new_column.ty);
    };

    let old_sequence = &old_sequences[old_sequence_idx];
    if old_sequence.same_definition(new_sequence, new_column.ty) {
        old_sequence.clone().with_column(new_sequence.column)
    } else {
        ModelSequence::new(new_sequence, new_column.ty)
    }
}

fn next_sequence_value(min: i128, max: i128, increment: i128, value: i128) -> i128 {
    let mut next = value + increment;
    if increment > 0 {
        if next > max {
            next = min + (next - max - 1) % (max - min + 1);
        }
    } else if next < min {
        next = max - (min - next - 1) % (max - min + 1);
    }
    next
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
    fn insert_would_violate_unique_constraint_projects_sequence_values() {
        let mut model = Model::new(keyed_payload_schema(true));
        model.committed_tables[0].rows.push(payload_row(1, "a"));

        assert!(model.insert_would_violate_unique_constraint(0, &payload_row(0, "b")));
    }

    #[test]
    fn begin_mut_tx_does_not_clone_committed_tables() {
        let mut model = Model::new(schema());
        model.committed_tables[0].rows.push(row(1));

        model.apply(&Interaction::BeginMutTx);

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
