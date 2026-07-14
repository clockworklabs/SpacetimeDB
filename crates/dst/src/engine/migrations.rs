use super::model::{ColumnDomain, Model};
use crate::schema::{
    ColumnPlan, IndexAlgorithm, IndexPlan, SchemaNames, SchemaPlan, SequencePlan, TablePlan, Type, UniqueConstraintPlan,
};

const MAX_SUM_VARIANTS: u8 = 32;
const MAX_EVENT_COLUMNS: usize = 32;
const MAX_TABLE_COLUMNS: usize = 32;
const MAX_TABLES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    schema: SchemaPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaRewrite {
    AddTable { is_event: bool },
    RemoveTable { table: String },
    AlterTable { table: String, ops: Vec<TableMigrationOp> },
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
        columns: Vec<usize>,
    },
    AddSequence {
        sequence: SequencePlan,
    },
    RemoveSequence {
        column: usize,
    },
    AddUniqueConstraint {
        columns: Vec<usize>,
    },
    RemoveUniqueConstraint {
        columns: Vec<usize>,
    },
    ChangePrimaryKey {
        column: Option<usize>,
    },
    ChangeIndex {
        columns: Vec<usize>,
    },
    ChangeColumnType {
        column: usize,
    },
    ReschemaEventTable,
}

impl Migration {
    pub(crate) fn from_schema(schema: SchemaPlan) -> Self {
        Self { schema }
    }

    #[cfg(test)]
    pub(crate) fn from_rewrites(base: &SchemaPlan, rewrites: Vec<SchemaRewrite>) -> anyhow::Result<Self> {
        let mut schema = base.clone();
        for rewrite in &rewrites {
            rewrite.apply_to(&mut schema)?;
        }
        Ok(Self::from_schema(schema))
    }

    pub(crate) fn schema(&self) -> &SchemaPlan {
        &self.schema
    }

    pub(crate) fn candidates(schema: &SchemaPlan, model: &Model) -> Vec<SchemaRewrite> {
        let mut candidates = Vec::new();
        let non_event_tables = schema.tables.iter().filter(|table| !table.is_event).count();

        if schema.tables.len() < MAX_TABLES {
            candidates.push(SchemaRewrite::AddTable { is_event: false });
            candidates.push(SchemaRewrite::AddTable { is_event: true });
        }

        for table_plan in &schema.tables {
            let row_count = model.row_count_by_table_name(&table_plan.name);
            let pristine = row_count == 0 && !model.ever_inserted_by_table_name(&table_plan.name);

            if (table_plan.is_event && row_count == 0) || (!table_plan.is_event && pristine && non_event_tables > 1) {
                candidates.push(SchemaRewrite::RemoveTable {
                    table: table_plan.name.clone(),
                });
            }

            if table_plan.is_event {
                if row_count == 0 && table_plan.columns.len() < MAX_EVENT_COLUMNS {
                    candidates.push(SchemaRewrite::alter_table(
                        table_plan,
                        [TableMigrationOp::ChangeAccess, TableMigrationOp::ReschemaEventTable],
                    ));
                }
                continue;
            }

            if table_plan.columns.len() < MAX_TABLE_COLUMNS {
                candidates.extend(Type::ALL.iter().copied().map(|ty| {
                    SchemaRewrite::alter_table(
                        table_plan,
                        [TableMigrationOp::ChangeAccess, TableMigrationOp::AddColumn { ty }],
                    )
                }));
            }

            if pristine {
                for sequence in addable_sequences(table_plan) {
                    candidates.push(SchemaRewrite::alter_table(
                        table_plan,
                        [TableMigrationOp::AddSequence { sequence }],
                    ));
                }

                for columns in addable_unique_constraint_columns(table_plan) {
                    let mut ops = Vec::new();
                    if !has_index(table_plan, &columns) {
                        ops.push(TableMigrationOp::AddIndex {
                            columns: columns.clone(),
                            algorithm: IndexAlgorithm::BTree,
                        });
                    }
                    ops.push(TableMigrationOp::AddUniqueConstraint { columns });
                    candidates.push(SchemaRewrite::alter_table(table_plan, ops));
                }
            }

            if row_count > 0 {
                for sequence in
                    addable_sequence_boundary_probes(table_plan, |column| column_domain(model, table_plan, column))
                {
                    candidates.push(SchemaRewrite::alter_table(
                        table_plan,
                        [TableMigrationOp::AddSequence { sequence }],
                    ));
                }
            }

            for column in removable_sequence_columns(table_plan) {
                candidates.push(SchemaRewrite::alter_table(
                    table_plan,
                    [TableMigrationOp::RemoveSequence { column }],
                ));
            }

            for columns in removable_unique_constraint_columns(table_plan) {
                candidates.push(SchemaRewrite::alter_table(
                    table_plan,
                    [TableMigrationOp::RemoveUniqueConstraint { columns }],
                ));
            }

            for (columns, algorithm) in addable_indexes(table_plan) {
                candidates.push(SchemaRewrite::alter_table(
                    table_plan,
                    [TableMigrationOp::AddIndex { columns, algorithm }],
                ));
            }

            for columns in changeable_index_columns(table_plan) {
                candidates.push(SchemaRewrite::alter_table(
                    table_plan,
                    [
                        TableMigrationOp::ChangeAccess,
                        TableMigrationOp::ChangeIndex {
                            columns: columns.clone(),
                        },
                    ],
                ));
                candidates.push(SchemaRewrite::alter_table(
                    table_plan,
                    [TableMigrationOp::RemoveIndex { columns }],
                ));
            }

            for column in widenable_sum_columns(table_plan) {
                candidates.push(SchemaRewrite::alter_table(
                    table_plan,
                    [
                        TableMigrationOp::ChangeAccess,
                        TableMigrationOp::ChangeColumnType { column },
                    ],
                ));

                if table_plan.primary_key.is_some() && table_plan.sequences.is_empty() {
                    candidates.push(SchemaRewrite::alter_table(
                        table_plan,
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
}

impl SchemaRewrite {
    fn alter_table(table: &TablePlan, ops: impl Into<Vec<TableMigrationOp>>) -> Self {
        Self::AlterTable {
            table: table.name.clone(),
            ops: ops.into(),
        }
    }

    pub(crate) fn apply_to(&self, schema: &mut SchemaPlan) -> anyhow::Result<()> {
        match self {
            Self::AddTable { is_event } => {
                schema.tables.push(new_table(&schema.tables, *is_event));
            }
            Self::RemoveTable { table } => {
                let table = table_position(schema, table)?;
                schema.tables.remove(table);
            }
            Self::AlterTable { table, ops } => {
                let table = table_position(schema, table)?;
                let table_plan = &mut schema.tables[table];
                for op in ops {
                    apply_table_op(table_plan, op.clone())?;
                }
            }
        }
        Ok(())
    }
}

fn table_position(schema: &SchemaPlan, table: &str) -> anyhow::Result<usize> {
    schema
        .tables
        .iter()
        .position(|table_plan| table_plan.name == table)
        .ok_or_else(|| anyhow::anyhow!("migration references missing table {table}"))
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
        TableMigrationOp::RemoveIndex { columns } => {
            ensure_columns_exist(table, &columns)?;
            let Some(index) = table.indexes.iter().position(|index| index.columns == columns) else {
                anyhow::bail!("remove-index migration references missing index on columns {columns:?}");
            };
            anyhow::ensure!(
                !is_required_index(table, &columns),
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
        TableMigrationOp::RemoveSequence { column } => {
            anyhow::ensure!(
                column < table.columns.len(),
                "remove-sequence migration references missing column"
            );
            let Some(sequence) = table.sequences.iter().position(|sequence| sequence.column == column) else {
                anyhow::bail!("remove-sequence migration references missing sequence on column {column}");
            };
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
        TableMigrationOp::RemoveUniqueConstraint { columns } => {
            ensure_columns_exist(table, &columns)?;
            let Some(constraint) = table
                .unique_constraints
                .iter()
                .position(|constraint| constraint.columns == columns)
            else {
                anyhow::bail!("remove-constraint migration references missing constraint on columns {columns:?}");
            };
            anyhow::ensure!(
                !table.primary_key.is_some_and(|primary_key| columns == [primary_key]),
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
        TableMigrationOp::ChangeIndex { columns } => {
            ensure_columns_exist(table, &columns)?;
            anyhow::ensure!(
                !is_required_index(table, &columns),
                "index migration selected a required index"
            );
            let Some(index_plan) = table.indexes.iter_mut().find(|index| index.columns == columns) else {
                anyhow::bail!("index migration references missing index on columns {columns:?}");
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

fn widenable_sum_columns(table: &TablePlan) -> Vec<usize> {
    table
        .columns
        .iter()
        .enumerate()
        .filter_map(|(column, column_plan)| match column_plan.ty {
            Type::Sum { variants } if variants < MAX_SUM_VARIANTS => Some(column),
            _ => None,
        })
        .collect()
}

fn addable_indexes(table: &TablePlan) -> Vec<(Vec<usize>, IndexAlgorithm)> {
    (0..table.columns.len())
        .filter_map(|column| {
            let columns = vec![column];
            (!has_index(table, &columns)).then_some((columns, IndexAlgorithm::BTree))
        })
        .collect()
}

fn changeable_index_columns(table: &TablePlan) -> Vec<Vec<usize>> {
    table
        .indexes
        .iter()
        .filter_map(|index| (!is_required_index(table, &index.columns)).then_some(index.columns.clone()))
        .collect()
}

fn addable_sequences(table: &TablePlan) -> Vec<SequencePlan> {
    table
        .columns
        .iter()
        .enumerate()
        .filter_map(|(column, column_plan)| {
            (column_plan.ty.is_integral() && table.sequences.iter().all(|sequence| sequence.column != column))
                .then(|| SequencePlan::new(column, column_plan.ty).expect("column type checked above"))
        })
        .collect()
}

fn addable_sequence_boundary_probes(
    table: &TablePlan,
    column_domain: impl Fn(usize) -> Option<ColumnDomain>,
) -> Vec<SequencePlan> {
    table
        .columns
        .iter()
        .enumerate()
        .filter_map(|(column, column_plan)| {
            let domain = column_domain(column)?;
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
        .collect()
}

fn column_domain(model: &Model, table: &TablePlan, column: usize) -> Option<ColumnDomain> {
    let column_name = &table.columns.get(column)?.name;
    model.column_domain_by_name(&table.name, column_name)
}

fn removable_sequence_columns(table: &TablePlan) -> Vec<usize> {
    table.sequences.iter().map(|sequence| sequence.column).collect()
}

fn addable_unique_constraint_columns(table: &TablePlan) -> Vec<Vec<usize>> {
    (0..table.columns.len())
        .filter_map(|column| {
            let columns = vec![column];
            (!table
                .unique_constraints
                .iter()
                .any(|constraint| constraint.columns == columns))
            .then_some(columns)
        })
        .collect()
}

fn removable_unique_constraint_columns(table: &TablePlan) -> Vec<Vec<usize>> {
    table
        .unique_constraints
        .iter()
        .filter_map(|constraint| {
            (!table
                .primary_key
                .is_some_and(|primary_key| constraint.columns == [primary_key]))
            .then_some(constraint.columns.clone())
        })
        .collect()
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
        "sum-widening migration selected maxed-out sum column"
    );
    *variants += 1;
    Ok(())
}
