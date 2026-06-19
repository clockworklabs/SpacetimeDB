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
    Replay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    BeganMutTx,
    Inserted { count_after: u64 },
    Deleted { count_after: u64 },
    Committed { summaries: Vec<TableSummary> },
    Counted { count: u64 },
    Replayed { summaries: Vec<TableSummary> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSummary {
    pub count: u64,
    pub hash: u64,
}

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

                if self.violates_unique_constraint_in(self.tables(), *table, row)
                    || self.tables()[*table].rows.contains(row)
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
                rows.push(row.clone());
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
                self.committed_tables = self.pending_tables.take().expect("active transaction");
                Observation::Committed {
                    summaries: self.summaries(),
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

    pub fn row_count(&self, table: usize) -> u64 {
        self.tables()[table].rows.len() as u64
    }

    pub fn summaries(&self) -> Vec<TableSummary> {
        self.tables().iter().map(|table| summarize_rows(&table.rows)).collect()
    }

    pub fn rows(&self, table: usize) -> &[Row] {
        &self.tables()[table].rows
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
            let coin = self.rng.next_u64() % 11;
            if coin == 0 {
                Interaction::Replay
            } else if coin < 6 {
                Interaction::Insert {
                    table: table_idx,
                    row: self.gen_row(&self.schema().tables[table_idx]),
                }
            } else if coin < 8 && !self.model.rows(table_idx).is_empty() {
                let rows = self.model.rows(table_idx);
                let row_index = self.rng.index(rows.len());
                Interaction::Delete {
                    table: table_idx,
                    row: rows[row_index].clone(),
                }
            } else if coin < 10 {
                Interaction::Count { table: table_idx }
            } else {
                Interaction::CommitTx
            }
        } else if self.rng.next_u64() % 5 == 0 {
            Interaction::Replay
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

pub fn summarize_rows(rows: &[Row]) -> TableSummary {
    let mut hash = 0u64;
    for row in rows {
        let row_hash = stable_hash(&row_to_bytes(row));
        hash = hash.wrapping_add(row_hash.rotate_left((row_hash & 31) as u32));
    }
    TableSummary {
        count: rows.len() as u64,
        hash,
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}
