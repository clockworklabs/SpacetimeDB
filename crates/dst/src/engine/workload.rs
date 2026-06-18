use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;

use crate::schema::{SchemaPlan, TablePlan, Type};

pub type Row = ProductValue;

#[derive(Debug, Clone)]
pub enum Interaction {
    BeginMutTx,
    Insert { table: usize, row: Row },
    Delete { table: usize, row: Row },
    CommitTx,
    Count { table: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    BeganMutTx,
    Inserted { count_after: u64 },
    Deleted { count_after: u64 },
    Committed,
    Counted { count: u64 },
}

#[derive(Debug)]
pub struct Model {
    schema: SchemaPlan,
    tables: Vec<TableState>,
    in_mut_tx: bool,
}

#[derive(Debug)]
struct TableState {
    rows: Vec<Row>,
}

impl Model {
    pub fn new(schema: SchemaPlan) -> Self {
        let tables = schema.tables.iter().map(|_| TableState { rows: vec![] }).collect();
        Self {
            schema,
            tables,
            in_mut_tx: false,
        }
    }

    fn violates_unique_constraint(&self, table: usize, row: &Row) -> bool {
        let table_plan = &self.schema.tables[table];
        let rows = &self.tables[table].rows;
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
                debug_assert!(!self.in_mut_tx);
                self.in_mut_tx = true;
                Observation::BeganMutTx
            }
            Interaction::Insert { table, row } => {
                debug_assert!(self.in_mut_tx);
                let table_plan = &self.schema.tables[*table];

                if self.violates_unique_constraint(*table, row) || self.tables[*table].rows.contains(row) {
                    return Observation::Inserted {
                        count_after: self.tables[*table].rows.len() as u64,
                    };
                }

                let rows = &mut self.tables[*table].rows;
                if let Some(pk_col) = table_plan.primary_key {
                    if let Some(pos) = rows.iter().position(|r| r.elements[pk_col] == row.elements[pk_col]) {
                        rows[pos] = row.clone();
                        return Observation::Inserted {
                            count_after: rows.len() as u64,
                        };
                    }
                }
                rows.push(row.clone());
                Observation::Inserted {
                    count_after: rows.len() as u64,
                }
            }
            Interaction::Delete { table, row } => {
                debug_assert!(self.in_mut_tx);
                let rows = &mut self.tables[*table].rows;
                rows.retain(|r| r != row);
                Observation::Deleted {
                    count_after: rows.len() as u64,
                }
            }
            Interaction::CommitTx => {
                debug_assert!(self.in_mut_tx);
                self.in_mut_tx = false;
                Observation::Committed
            }
            Interaction::Count { table } => {
                debug_assert!(self.in_mut_tx);
                Observation::Counted {
                    count: self.tables[*table].rows.len() as u64,
                }
            }
        }
    }

    pub fn in_mut_tx(&self) -> bool {
        self.in_mut_tx
    }

    pub fn row_count(&self, table: usize) -> u64 {
        self.tables[table].rows.len() as u64
    }

    pub fn rows(&self, table: usize) -> &[Row] {
        &self.tables[table].rows
    }
}

pub struct WorkloadGen {
    rng: Rng,
    model: Model,
}

impl WorkloadGen {
    pub fn new(rng: Rng, model: Model) -> Self {
        Self { rng, model }
    }

    fn schema(&self) -> &SchemaPlan {
        &self.model.schema
    }

    fn gen_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(self.rng.next_u64() % 2 == 0),
            Type::I64 => AlgebraicValue::I64(self.rng.next_u64() as i64),
            Type::U64 => AlgebraicValue::U64(self.rng.next_u64()),
            Type::String => AlgebraicValue::String(format!("v_{}", self.rng.next_u64()).into()),
            Type::Bytes => {
                let len = (self.rng.next_u64() % 16) as usize;
                let bytes: Vec<u8> = (0..len).map(|_| self.rng.next_u64() as u8).collect();
                AlgebraicValue::Array(ArrayValue::U8(bytes.into()))
            }
        }
    }

    fn gen_row(&self, table: &TablePlan) -> Row {
        table
            .columns
            .iter()
            .map(|c| self.gen_value(c.ty))
            .collect::<ProductValue>()
    }

    pub fn next_interaction(&mut self) -> Interaction {
        let table_idx = self.rng.index(self.schema().tables.len());

        let interaction = if self.model.in_mut_tx() {
            let coin = self.rng.next_u64() % 10;
            if coin < 5 {
                Interaction::Insert {
                    table: table_idx,
                    row: self.gen_row(&self.schema().tables[table_idx]),
                }
            } else if coin < 7 && !self.model.rows(table_idx).is_empty() {
                let rows = self.model.rows(table_idx);
                let row_index = self.rng.index(rows.len());
                Interaction::Delete {
                    table: table_idx,
                    row: rows[row_index].clone(),
                }
            } else if coin < 9 {
                Interaction::Count { table: table_idx }
            } else {
                Interaction::CommitTx
            }
        } else {
            Interaction::BeginMutTx
        };

        self.model.apply(&interaction);
        interaction
    }
}
impl Iterator for WorkloadGen {
    type Item = Interaction;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_interaction())
    }
}

use spacetimedb_sats::ArrayValue;

pub fn row_to_bytes(row: &Row) -> Vec<u8> {
    to_vec(row).expect("row serialization must not fail")
}
