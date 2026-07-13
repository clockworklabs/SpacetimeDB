use super::model::{ColumnDomain, Model};
use crate::schema::{
    ColumnPlan, IndexAlgorithm, IndexPlan, SchemaNames, SchemaPlan, SequencePlan, TablePlan, Type, UniqueConstraintPlan,
};
use spacetimedb_lib::AlgebraicValue;

const MAX_SUM_VARIANTS: u8 = 32;
const MAX_EVENT_COLUMNS: usize = 32;
const MAX_TABLE_COLUMNS: usize = 32;
const MAX_TABLES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migration {
    AddTable { is_event: bool },
    RemoveTable { table: usize },
    AlterTable { table: usize, ops: Vec<TableMigrationOp> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableMigrationOp {
    ChangeAccess,
    AddColumn {
        ty: Type,
    },
    AddIndex {
        columns: Vec<usize>,
        algorithm: IndexAlgorithm,
    },
    RemoveIndex {
        index: usize,
    },
    AddSequence {
        sequence: SequencePlan,
    },
    RemoveSequence {
        sequence: usize,
    },
    AddUniqueConstraint {
        columns: Vec<usize>,
    },
    RemoveUniqueConstraint {
        constraint: usize,
    },
    ChangePrimaryKey {
        column: Option<usize>,
    },
    ChangeIndex {
        index: usize,
    },
    ChangeColumnType {
        column: usize,
    },
    ReschemaEventTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExpectedStep {
    AddTable,
    RemoveTable,
    AddColumns,
    AddIndex,
    RemoveIndex,
    AddSequence,
    RemoveSequence,
    AddConstraint,
    RemoveConstraint,
    ChangeAccess,
    ChangePrimaryKey,
    ChangeColumns,
    ReschemaEventTable,
    DisconnectAllUsers,
}

impl Migration {
    pub(crate) fn candidates(model: &Model) -> Vec<Self> {
        let schema = model.schema();
        let mut candidates = Vec::new();
        let non_event_tables = schema.tables.iter().filter(|table| !table.is_event).count();

        if schema.tables.len() < MAX_TABLES {
            candidates.push(Self::AddTable { is_event: false });
            candidates.push(Self::AddTable { is_event: true });
        }

        for (table, table_plan) in schema.tables.iter().enumerate() {
            let row_count = model.row_count(table);
            let pristine = row_count == 0 && !model.ever_inserted(table);

            if (table_plan.is_event && row_count == 0) || (!table_plan.is_event && pristine && non_event_tables > 1) {
                candidates.push(Self::RemoveTable { table });
            }

            if table_plan.is_event {
                if row_count == 0 && table_plan.columns.len() < MAX_EVENT_COLUMNS {
                    candidates.push(Self::alter_table(
                        table,
                        [TableMigrationOp::ChangeAccess, TableMigrationOp::ReschemaEventTable],
                    ));
                }
                continue;
            }

            if table_plan.columns.len() < MAX_TABLE_COLUMNS {
                candidates.extend(Type::ALL.iter().copied().map(|ty| {
                    Self::alter_table(
                        table,
                        [TableMigrationOp::ChangeAccess, TableMigrationOp::AddColumn { ty }],
                    )
                }));
            }

            if pristine {
                if let Some(sequence) = first_addable_sequence(table_plan) {
                    candidates.push(Self::alter_table(table, [TableMigrationOp::AddSequence { sequence }]));
                }

                if let Some(columns) = first_addable_unique_constraint_columns(table_plan) {
                    let mut ops = Vec::new();
                    if !has_index(table_plan, &columns) {
                        ops.push(TableMigrationOp::AddIndex {
                            columns: columns.clone(),
                            algorithm: IndexAlgorithm::BTree,
                        });
                    }
                    ops.push(TableMigrationOp::AddUniqueConstraint { columns });
                    candidates.push(Self::alter_table(table, ops));
                }
            }

            if row_count > 0 {
                if let Some(sequence) =
                    first_addable_sequence_boundary_probe(table_plan, |column| model.column_domain(table, column))
                {
                    candidates.push(Self::alter_table(table, [TableMigrationOp::AddSequence { sequence }]));
                }
            }

            if let Some(sequence) = first_removable_sequence(table_plan) {
                candidates.push(Self::alter_table(
                    table,
                    [TableMigrationOp::RemoveSequence { sequence }],
                ));
            }

            if let Some(constraint) = first_removable_unique_constraint(table_plan) {
                candidates.push(Self::alter_table(
                    table,
                    [TableMigrationOp::RemoveUniqueConstraint { constraint }],
                ));
            }

            if let Some((columns, algorithm)) = first_addable_index(table_plan) {
                candidates.push(Self::alter_table(
                    table,
                    [TableMigrationOp::AddIndex { columns, algorithm }],
                ));
            }

            if let Some(index) = first_changeable_index(table_plan) {
                candidates.push(Self::alter_table(
                    table,
                    [TableMigrationOp::ChangeAccess, TableMigrationOp::ChangeIndex { index }],
                ));
                candidates.push(Self::alter_table(table, [TableMigrationOp::RemoveIndex { index }]));
            }

            if let Some(column) = first_widenable_sum_column(table_plan) {
                candidates.push(Self::alter_table(
                    table,
                    [
                        TableMigrationOp::ChangeAccess,
                        TableMigrationOp::ChangeColumnType { column },
                    ],
                ));

                if table_plan.primary_key.is_some() && table_plan.sequences.is_empty() {
                    candidates.push(Self::alter_table(
                        table,
                        [
                            TableMigrationOp::ChangePrimaryKey { column: None },
                            TableMigrationOp::ChangeColumnType { column },
                        ],
                    ));
                }
            }
        }

        candidates
    }

    pub fn apply_to(&self, schema: &SchemaPlan) -> anyhow::Result<SchemaPlan> {
        let mut next = schema.clone();

        match self {
            Self::AddTable { is_event } => {
                next.tables.push(new_table(&next.tables, *is_event));
            }
            Self::RemoveTable { table } => {
                anyhow::ensure!(
                    *table < next.tables.len(),
                    "remove-table migration references missing table {table}"
                );
                next.tables.remove(*table);
            }
            Self::AlterTable { table, ops } => {
                let table_plan = next
                    .tables
                    .get_mut(*table)
                    .ok_or_else(|| anyhow::anyhow!("migration references missing table {table}"))?;

                for op in ops {
                    apply_table_op(table_plan, op.clone())?;
                }
            }
        }

        Ok(next)
    }

    pub fn added_column_defaults(&self) -> Option<(usize, Vec<AlgebraicValue>)> {
        let Self::AlterTable { table, ops } = self else {
            return None;
        };
        let defaults = ops
            .iter()
            .filter_map(|op| match op {
                TableMigrationOp::AddColumn { ty } => Some(ty.default_value()),
                _ => None,
            })
            .collect::<Vec<_>>();

        (!defaults.is_empty()).then_some((*table, defaults))
    }

    pub fn expected_steps(&self) -> Vec<ExpectedStep> {
        let mut expected = Vec::new();

        match self {
            Self::AddTable { .. } => expected.push(ExpectedStep::AddTable),
            Self::RemoveTable { .. } => {
                expected.push(ExpectedStep::RemoveTable);
                expected.push(ExpectedStep::DisconnectAllUsers);
            }
            Self::AlterTable { ops, .. } => {
                for op in ops {
                    expected_steps_for_table_op(op, &mut expected);
                }
            }
        }

        if expected.contains(&ExpectedStep::AddColumns) {
            expected.retain(|step| *step != ExpectedStep::ChangeColumns);
        }

        expected.sort();
        expected.dedup();
        expected
    }

    fn alter_table(table: usize, ops: impl Into<Vec<TableMigrationOp>>) -> Self {
        Self::AlterTable { table, ops: ops.into() }
    }
}

fn expected_steps_for_table_op(op: &TableMigrationOp, expected: &mut Vec<ExpectedStep>) {
    match op {
        TableMigrationOp::ChangeAccess => expected.push(ExpectedStep::ChangeAccess),
        TableMigrationOp::AddColumn { .. } => {
            expected.push(ExpectedStep::AddColumns);
            expected.push(ExpectedStep::DisconnectAllUsers);
        }
        TableMigrationOp::AddIndex { .. } => expected.push(ExpectedStep::AddIndex),
        TableMigrationOp::RemoveIndex { .. } => expected.push(ExpectedStep::RemoveIndex),
        TableMigrationOp::AddSequence { .. } => expected.push(ExpectedStep::AddSequence),
        TableMigrationOp::RemoveSequence { .. } => expected.push(ExpectedStep::RemoveSequence),
        TableMigrationOp::AddUniqueConstraint { .. } => expected.push(ExpectedStep::AddConstraint),
        TableMigrationOp::RemoveUniqueConstraint { .. } => expected.push(ExpectedStep::RemoveConstraint),
        TableMigrationOp::ChangePrimaryKey { .. } => expected.push(ExpectedStep::ChangePrimaryKey),
        TableMigrationOp::ChangeIndex { .. } => {
            expected.push(ExpectedStep::RemoveIndex);
            expected.push(ExpectedStep::AddIndex);
        }
        TableMigrationOp::ChangeColumnType { .. } => expected.push(ExpectedStep::ChangeColumns),
        TableMigrationOp::ReschemaEventTable => {
            expected.push(ExpectedStep::ReschemaEventTable);
            expected.push(ExpectedStep::DisconnectAllUsers);
        }
    }
}

fn apply_table_op(table: &mut TablePlan, op: TableMigrationOp) -> anyhow::Result<()> {
    match op {
        TableMigrationOp::ChangeAccess => {
            table.is_public = !table.is_public;
        }
        TableMigrationOp::AddColumn { ty } => {
            anyhow::ensure!(!table.is_event, "add-column migration selected event table");
            anyhow::ensure!(
                table.columns.len() < MAX_TABLE_COLUMNS,
                "table already has the configured maximum number of columns"
            );
            table.columns.push(ColumnPlan {
                name: SchemaNames::fresh_column_name(table, "added_col"),
                ty,
            });
        }
        TableMigrationOp::AddIndex { columns, algorithm } => {
            ensure_columns_exist(table, &columns)?;
            anyhow::ensure!(
                !has_index(table, &columns),
                "add-index migration selected existing index columns"
            );
            table.indexes.push(IndexPlan { columns, algorithm });
        }
        TableMigrationOp::RemoveIndex { index } => {
            let Some(index_plan) = table.indexes.get(index) else {
                anyhow::bail!("remove-index migration references missing index {index}");
            };
            anyhow::ensure!(
                !is_required_index(table, &index_plan.columns),
                "remove-index migration selected a required index"
            );
            table.indexes.remove(index);
        }
        TableMigrationOp::AddSequence { sequence } => {
            let column = sequence.column;
            let Some(column_plan) = table.columns.get(column) else {
                anyhow::bail!("add-sequence migration references missing column {column}");
            };
            anyhow::ensure!(
                column_plan.ty.is_integral(),
                "add-sequence migration selected non-integral column"
            );
            anyhow::ensure!(
                table.sequences.iter().all(|existing| existing.column != column),
                "add-sequence migration selected an already sequenced column"
            );
            table.sequences.push(sequence);
        }
        TableMigrationOp::RemoveSequence { sequence } => {
            anyhow::ensure!(
                sequence < table.sequences.len(),
                "remove-sequence migration references missing sequence"
            );
            table.sequences.remove(sequence);
        }
        TableMigrationOp::AddUniqueConstraint { columns } => {
            ensure_columns_exist(table, &columns)?;
            anyhow::ensure!(
                !table
                    .unique_constraints
                    .iter()
                    .any(|constraint| constraint.columns == columns),
                "add-constraint migration selected existing constraint columns"
            );
            anyhow::ensure!(
                has_index(table, &columns),
                "add-constraint migration requires a matching index"
            );
            table.unique_constraints.push(UniqueConstraintPlan { columns });
        }
        TableMigrationOp::RemoveUniqueConstraint { constraint } => {
            let Some(constraint_plan) = table.unique_constraints.get(constraint) else {
                anyhow::bail!("remove-constraint migration references missing constraint {constraint}");
            };
            anyhow::ensure!(
                !table
                    .primary_key
                    .is_some_and(|primary_key| constraint_plan.columns == [primary_key]),
                "remove-constraint migration selected primary-key constraint"
            );
            table.unique_constraints.remove(constraint);
        }
        TableMigrationOp::ChangePrimaryKey { column } => {
            if let Some(column) = column {
                anyhow::ensure!(
                    column < table.columns.len(),
                    "primary-key migration references missing column"
                );
            }
            table.primary_key = column;
        }
        TableMigrationOp::ChangeIndex { index } => {
            let Some(index_plan) = table.indexes.get_mut(index) else {
                anyhow::bail!("index migration references missing index {index}");
            };
            index_plan.algorithm = match index_plan.algorithm {
                IndexAlgorithm::BTree => IndexAlgorithm::Hash,
                IndexAlgorithm::Hash => IndexAlgorithm::BTree,
            };
        }
        TableMigrationOp::ChangeColumnType { column } => {
            widen_sum_column(table, Some(column))?;
        }
        TableMigrationOp::ReschemaEventTable => {
            anyhow::ensure!(table.is_event, "event-table migration selected non-event table");
            anyhow::ensure!(
                table.columns.len() < MAX_EVENT_COLUMNS,
                "event table already has the configured maximum number of columns"
            );
            table.columns.push(ColumnPlan {
                name: SchemaNames::fresh_column_name(table, "reschema_payload"),
                ty: Type::U64,
            });
        }
    }

    Ok(())
}

fn new_table(tables: &[TablePlan], is_event: bool) -> TablePlan {
    if is_event {
        return TablePlan {
            name: SchemaNames::fresh_table_name(tables, "added_events"),
            columns: vec![ColumnPlan {
                name: "payload".into(),
                ty: Type::U64,
            }],
            primary_key: None,
            indexes: vec![],
            unique_constraints: vec![],
            sequences: vec![],
            is_public: true,
            is_event: true,
        };
    }

    TablePlan {
        name: SchemaNames::fresh_table_name(tables, "added_table"),
        columns: vec![
            ColumnPlan {
                name: "id".into(),
                ty: Type::U64,
            },
            ColumnPlan {
                name: "value".into(),
                ty: Type::U64,
            },
            ColumnPlan {
                name: "kind".into(),
                ty: Type::U64,
            },
        ],
        primary_key: Some(0),
        indexes: vec![
            IndexPlan {
                columns: vec![0],
                algorithm: IndexAlgorithm::BTree,
            },
            IndexPlan {
                columns: vec![1],
                algorithm: IndexAlgorithm::Hash,
            },
        ],
        unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
        sequences: vec![SequencePlan::new(0, Type::U64).expect("u64 is integral")],
        is_public: true,
        is_event: false,
    }
}

fn first_widenable_sum_column(table: &TablePlan) -> Option<usize> {
    table.columns.iter().position(|column| match column.ty {
        Type::Sum { variants } => variants < MAX_SUM_VARIANTS,
        _ => false,
    })
}

fn first_addable_index(table: &TablePlan) -> Option<(Vec<usize>, IndexAlgorithm)> {
    (0..table.columns.len()).find_map(|column| {
        let columns = vec![column];
        (!has_index(table, &columns)).then_some((columns, IndexAlgorithm::BTree))
    })
}

fn first_changeable_index(table: &TablePlan) -> Option<usize> {
    table
        .indexes
        .iter()
        .position(|index| !is_required_index(table, &index.columns))
}

fn first_addable_sequence(table: &TablePlan) -> Option<SequencePlan> {
    table.columns.iter().enumerate().find_map(|(column, column_plan)| {
        (column_plan.ty.is_integral() && table.sequences.iter().all(|sequence| sequence.column != column))
            .then(|| SequencePlan::new(column, column_plan.ty).expect("column type checked above"))
    })
}

fn first_addable_sequence_boundary_probe(
    table: &TablePlan,
    column_domain: impl Fn(usize) -> ColumnDomain,
) -> Option<SequencePlan> {
    table.columns.iter().enumerate().find_map(|(column, column_plan)| {
        let domain = column_domain(column);
        if column_plan.ty != Type::U64
            || domain.sequenced
            || !domain.single_column_indexed
            || !domain.single_column_unique
        {
            return None;
        }

        let max_value = domain.positive_i128_value_above(2)?;
        SequencePlan::with_existing_value_as_max(column, column_plan.ty, max_value)
    })
}

fn first_removable_sequence(table: &TablePlan) -> Option<usize> {
    (!table.sequences.is_empty()).then_some(0)
}

fn first_addable_unique_constraint_columns(table: &TablePlan) -> Option<Vec<usize>> {
    (0..table.columns.len()).find_map(|column| {
        let columns = vec![column];
        (!table
            .unique_constraints
            .iter()
            .any(|constraint| constraint.columns == columns))
        .then_some(columns)
    })
}

fn first_removable_unique_constraint(table: &TablePlan) -> Option<usize> {
    table.unique_constraints.iter().position(|constraint| {
        !table
            .primary_key
            .is_some_and(|primary_key| constraint.columns == [primary_key])
    })
}

fn has_index(table: &TablePlan, columns: &[usize]) -> bool {
    table.indexes.iter().any(|index| index.columns == columns)
}

fn is_required_index(table: &TablePlan, columns: &[usize]) -> bool {
    table.primary_key.is_some_and(|primary_key| columns == [primary_key])
        || table
            .unique_constraints
            .iter()
            .any(|constraint| constraint.columns == columns)
}

fn ensure_columns_exist(table: &TablePlan, columns: &[usize]) -> anyhow::Result<()> {
    anyhow::ensure!(!columns.is_empty(), "migration selected empty column list");
    anyhow::ensure!(
        columns.iter().all(|&column| column < table.columns.len()),
        "migration references missing column"
    );
    Ok(())
}

fn widen_sum_column(table: &mut TablePlan, column: Option<usize>) -> anyhow::Result<()> {
    let column = column.ok_or_else(|| anyhow::anyhow!("sum-widening migration missing column"))?;
    let Some(column_plan) = table.columns.get_mut(column) else {
        anyhow::bail!("sum-widening migration references missing column {column}");
    };

    let Type::Sum { variants } = &mut column_plan.ty else {
        anyhow::bail!("sum-widening migration selected a non-sum column");
    };
    anyhow::ensure!(
        *variants < MAX_SUM_VARIANTS,
        "sum column already has the configured maximum number of variants"
    );
    *variants += 1;
    Ok(())
}
