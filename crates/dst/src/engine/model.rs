use super::workload::{summarize_rows, CommitDelta, Interaction, Observation, Row, TableDelta, TableSummary};
use crate::schema::SchemaPlan;

#[derive(Debug)]
pub struct Model {
    schema: SchemaPlan,
    committed_tables: Vec<TableState>,
    pending_tables: Option<Vec<TableState>>,
}

#[derive(Debug, Clone)]
struct TableState {
    rows: Vec<Row>,
}

impl Model {
    pub fn new(schema: SchemaPlan) -> Self {
        let committed_tables = schema.tables.iter().map(|_| TableState { rows: vec![] }).collect();
        Self {
            schema,
            committed_tables,
            pending_tables: None,
        }
    }

    pub fn schema(&self) -> &SchemaPlan {
        &self.schema
    }

    fn tables(&self) -> &[TableState] {
        self.pending_tables.as_deref().unwrap_or(&self.committed_tables)
    }

    fn pending_tables_mut(&mut self) -> &mut [TableState] {
        self.pending_tables
            .as_deref_mut()
            .expect("mutable interaction without active transaction")
    }

    fn violates_unique_constraint_in(&self, tables: &[TableState], table: usize, row: &Row) -> bool {
        let table_plan = &self.schema.tables[table];
        let rows = &tables[table].rows;
        for constraint in &table_plan.unique_constraints {
            if rows
                .iter()
                .any(|r| constraint.columns.iter().all(|&c| r.elements[c] == row.elements[c]))
            {
                return true;
            }
        }
        false
    }

    pub fn apply(&mut self, interaction: &Interaction) -> Observation {
        match interaction {
            Interaction::BeginMutTx => {
                debug_assert!(self.pending_tables.is_none());
                self.pending_tables = Some(self.committed_tables.clone());
                Observation::BeganMutTx
            }
            Interaction::Insert { table, row } => {
                debug_assert!(self.pending_tables.is_some());
                let primary_key = self.schema.tables[*table].primary_key;
                let row = row.clone();

                if self.violates_unique_constraint_in(self.tables(), *table, &row)
                    || self.tables()[*table].rows.contains(&row)
                {
                    return Observation::Inserted {
                        count_after: self.tables()[*table].rows.len() as u64,
                    };
                }

                let rows = &mut self.pending_tables_mut()[*table].rows;
                if let Some(pk_col) = primary_key {
                    if let Some(pos) = rows.iter().position(|r| r.elements[pk_col] == row.elements[pk_col]) {
                        rows[pos] = row.clone();
                        return Observation::Inserted {
                            count_after: rows.len() as u64,
                        };
                    }
                }
                rows.push(row);
                Observation::Inserted {
                    count_after: rows.len() as u64,
                }
            }
            Interaction::Delete { table, row } => {
                debug_assert!(self.pending_tables.is_some());
                let rows = &mut self.pending_tables_mut()[*table].rows;
                rows.retain(|r| r != row);
                Observation::Deleted {
                    count_after: rows.len() as u64,
                }
            }
            Interaction::CommitTx => {
                debug_assert!(self.pending_tables.is_some());
                let pending_tables = self.pending_tables.take().expect("active transaction");
                let delta = commit_delta_from_tables(&self.committed_tables, &pending_tables);
                self.committed_tables = pending_tables;
                Observation::Committed {
                    delta,
                    auto_inc_values: vec![],
                }
            }
            Interaction::Count { table } => {
                debug_assert!(self.pending_tables.is_some());
                Observation::Counted {
                    count: self.tables()[*table].rows.len() as u64,
                }
            }
            Interaction::Replay => {
                self.pending_tables = None;
                Observation::Replayed {
                    summaries: self.summaries(),
                }
            }
        }
    }

    pub fn in_mut_tx(&self) -> bool {
        self.pending_tables.is_some()
    }

    pub fn summaries(&self) -> Vec<TableSummary> {
        self.tables().iter().map(|table| summarize_rows(&table.rows)).collect()
    }

    pub fn rows(&self, table: usize) -> &[Row] {
        &self.tables()[table].rows
    }
}

fn commit_delta_from_tables(before: &[TableState], after: &[TableState]) -> CommitDelta {
    let mut tables = Vec::new();

    for (table, (before, after)) in before.iter().zip(after).enumerate() {
        let inserts = rows_absent_from(&after.rows, &before.rows);
        let deletes = rows_absent_from(&before.rows, &after.rows);
        let truncated = !before.rows.is_empty() && after.rows.is_empty() && !deletes.is_empty();

        if inserts.is_empty() && deletes.is_empty() && !truncated {
            continue;
        }

        tables.push(TableDelta {
            table,
            inserts: summarize_rows(&inserts),
            deletes: summarize_rows(&deletes),
            truncated,
        });
    }

    CommitDelta { tables }
}

fn rows_absent_from(rows: &[Row], other: &[Row]) -> Vec<Row> {
    rows.iter().filter(|row| !other.contains(row)).cloned().collect()
}
