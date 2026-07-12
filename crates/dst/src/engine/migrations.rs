use spacetimedb_lib::AlgebraicValue;
use spacetimedb_runtime::sim::Rng;

use crate::schema::{ColumnPlan, IndexAlgorithm, SchemaDecisions, SchemaNames, SchemaPlan, TablePlan, Type};

const MAX_SUM_VARIANTS: u8 = 32;
const MAX_EVENT_COLUMNS: usize = 32;
const MAX_TABLE_COLUMNS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub table: usize,
    pub ops: Vec<MigrationOp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOp {
    ChangeAccess,
    AddColumn { ty: Type },
    ChangePrimaryKey { column: Option<usize> },
    ChangeIndex { index: usize },
    ChangeColumnType { column: usize },
    ReschemaEventTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExpectedStep {
    AddColumns,
    AddIndex,
    RemoveIndex,
    ChangeAccess,
    ChangePrimaryKey,
    ChangeColumns,
    ReschemaEventTable,
    DisconnectAllUsers,
}

impl Migration {
    pub fn choose(schema: &SchemaPlan, rng: &Rng, table_row_count: impl Fn(usize) -> usize) -> Option<Self> {
        let candidates = Self::candidates(schema, table_row_count);
        SchemaDecisions::choose_index(rng, candidates.len()).map(|idx| candidates[idx].clone())
    }

    pub fn candidates(schema: &SchemaPlan, table_row_count: impl Fn(usize) -> usize) -> Vec<Self> {
        let mut candidates = Vec::new();

        for (table, table_plan) in schema.tables.iter().enumerate() {
            if table_plan.is_event {
                if table_row_count(table) == 0 && table_plan.columns.len() < MAX_EVENT_COLUMNS {
                    candidates.push(Self::new(
                        table,
                        [MigrationOp::ChangeAccess, MigrationOp::ReschemaEventTable],
                    ));
                }
                continue;
            }

            if table_plan.columns.len() < MAX_TABLE_COLUMNS {
                candidates.extend(
                    Type::ALL
                        .iter()
                        .copied()
                        .map(|ty| Self::new(table, [MigrationOp::ChangeAccess, MigrationOp::AddColumn { ty }])),
                );
            }

            if let Some(column) = first_widenable_sum_column(table_plan) {
                candidates.push(Self::new(
                    table,
                    [MigrationOp::ChangeAccess, MigrationOp::ChangeColumnType { column }],
                ));

                if table_plan.primary_key.is_some() && table_plan.sequences.is_empty() {
                    candidates.push(Self::new(
                        table,
                        [
                            MigrationOp::ChangePrimaryKey { column: None },
                            MigrationOp::ChangeColumnType { column },
                        ],
                    ));
                }
            }

            if let Some(index) = first_changeable_index(table_plan) {
                candidates.push(Self::new(
                    table,
                    [MigrationOp::ChangeAccess, MigrationOp::ChangeIndex { index }],
                ));
            }
        }

        candidates
    }

    pub fn apply_to(&self, schema: &SchemaPlan) -> anyhow::Result<SchemaPlan> {
        let mut next = schema.clone();
        let table = next
            .tables
            .get_mut(self.table)
            .ok_or_else(|| anyhow::anyhow!("migration references missing table {}", self.table))?;

        for op in &self.ops {
            apply_op(table, *op)?;
        }

        Ok(next)
    }

    pub fn added_column_defaults(&self) -> Vec<AlgebraicValue> {
        self.ops
            .iter()
            .filter_map(|op| match op {
                MigrationOp::AddColumn { ty } => Some(ty.default_value()),
                _ => None,
            })
            .collect()
    }

    pub fn expected_steps(&self) -> Vec<ExpectedStep> {
        let mut expected = Vec::new();

        for op in &self.ops {
            match op {
                MigrationOp::ChangeAccess => expected.push(ExpectedStep::ChangeAccess),
                MigrationOp::AddColumn { .. } => {
                    expected.push(ExpectedStep::AddColumns);
                    expected.push(ExpectedStep::DisconnectAllUsers);
                }
                MigrationOp::ChangePrimaryKey { .. } => expected.push(ExpectedStep::ChangePrimaryKey),
                MigrationOp::ChangeIndex { .. } => {
                    expected.push(ExpectedStep::RemoveIndex);
                    expected.push(ExpectedStep::AddIndex);
                }
                MigrationOp::ChangeColumnType { .. } => expected.push(ExpectedStep::ChangeColumns),
                MigrationOp::ReschemaEventTable => {
                    expected.push(ExpectedStep::ReschemaEventTable);
                    expected.push(ExpectedStep::DisconnectAllUsers);
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

    fn new(table: usize, ops: impl Into<Vec<MigrationOp>>) -> Self {
        Self { table, ops: ops.into() }
    }
}

fn apply_op(table: &mut TablePlan, op: MigrationOp) -> anyhow::Result<()> {
    match op {
        MigrationOp::ChangeAccess => {
            table.is_public = !table.is_public;
        }
        MigrationOp::AddColumn { ty } => {
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
        MigrationOp::ChangePrimaryKey { column } => {
            if let Some(column) = column {
                anyhow::ensure!(
                    column < table.columns.len(),
                    "primary-key migration references missing column"
                );
            }
            table.primary_key = column;
        }
        MigrationOp::ChangeIndex { index } => {
            let Some(index_plan) = table.indexes.get_mut(index) else {
                anyhow::bail!("index migration references missing index {index}");
            };
            index_plan.algorithm = match index_plan.algorithm {
                IndexAlgorithm::BTree => IndexAlgorithm::Hash,
                IndexAlgorithm::Hash => IndexAlgorithm::BTree,
            };
        }
        MigrationOp::ChangeColumnType { column } => {
            widen_sum_column(table, Some(column))?;
        }
        MigrationOp::ReschemaEventTable => {
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

fn first_widenable_sum_column(table: &TablePlan) -> Option<usize> {
    table.columns.iter().position(|column| match column.ty {
        Type::Sum { variants } => variants < MAX_SUM_VARIANTS,
        _ => false,
    })
}

fn first_changeable_index(table: &TablePlan) -> Option<usize> {
    table
        .indexes
        .iter()
        .position(|index| !is_required_index(table, &index.columns))
}

fn is_required_index(table: &TablePlan, columns: &[usize]) -> bool {
    table.primary_key.is_some_and(|primary_key| columns == [primary_key])
        || table
            .unique_constraints
            .iter()
            .any(|constraint| constraint.columns == columns)
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
