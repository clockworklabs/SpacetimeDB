use std::fmt::{Debug, Error, Formatter};

use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::ArrayValue;

use super::model::Model;
use crate::schema::{SchemaPlan, TablePlan, Type};

pub type Row = ProductValue;

#[derive(Debug, Clone)]
pub enum Interaction {
    BeginMutTx,
    Insert { table: usize, row: Row },
    Delete { table: usize, row: Row },
    CommitTx,
    Replay,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionCounts {
    pub total: usize,
    pub begin_mut_tx: usize,
    pub insert: usize,
    pub delete: usize,
    pub commit_tx: usize,
    pub replay: usize,
}

impl InteractionCounts {
    pub fn record(&mut self, interaction: &Interaction) {
        self.total += 1;

        match interaction {
            Interaction::BeginMutTx => self.begin_mut_tx += 1,
            Interaction::Insert { .. } => self.insert += 1,
            Interaction::Delete { .. } => self.delete += 1,
            Interaction::CommitTx => self.commit_tx += 1,
            Interaction::Replay => self.replay += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    BeganMutTx,
    Inserted { outcome: InsertOutcome },
    Deleted,
    Committed { delta: CommitDelta },
    Replayed { state: CountState },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Accepted(Row),
    UniqueConstraintViolation,
}

#[derive(Debug, Clone, Copy)]
pub struct InteractionWeights {
    pub insert: u64,
    pub delete: u64,
    pub commit_tx: u64,
    pub replay: u64,
}

impl Default for InteractionWeights {
    fn default() -> Self {
        Self {
            insert: 50,
            delete: 20,
            commit_tx: 29,
            replay: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum InteractionChoice {
    Insert,
    Delete,
    CommitTx,
    Replay,
}

pub struct WorkloadGen {
    rng: Rng,
    model: Model,
    stats: InteractionCounts,
    weights: InteractionWeights,
}

impl WorkloadGen {
    pub fn new(rng: Rng, model: Model) -> Self {
        Self {
            rng,
            model,
            stats: InteractionCounts::default(),
            weights: InteractionWeights::default(),
        }
    }

    pub fn stats(&self) -> InteractionCounts {
        self.stats
    }

    fn schema(&self) -> &SchemaPlan {
        self.model.schema()
    }

    fn gen_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(self.rng.next_u64().is_multiple_of(2)),
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

    fn gen_insert_row(&self, table_idx: usize) -> Row {
        let table = &self.schema().tables[table_idx];
        let mut row = self.gen_row(table);

        if let Some(sequence) = table.sequences.first() {
            row.elements[sequence.column] = match table.columns[sequence.column].ty {
                Type::I64 => AlgebraicValue::I64(0),
                Type::U64 => AlgebraicValue::U64(0),
                _ => unreachable!("sequence columns are integral"),
            };
        }

        row
    }

    fn non_auto_inc_table_idx(&self) -> Option<usize> {
        let auto_inc_table = self
            .schema()
            .auto_inc_table_and_column()
            .map(|(table_idx, _)| table_idx);

        (0..self.schema().tables.len()).find(|&table_idx| Some(table_idx) != auto_inc_table)
    }

    pub fn next_interaction(&mut self) -> Interaction {
        let choice = self.pick_interaction_choice();
        let interaction = self.interaction_from_choice(choice);

        self.model.apply(&interaction);
        self.stats.record(&interaction);

        interaction
    }

    fn interaction_from_choice(&mut self, choice: InteractionChoice) -> Interaction {
        if !self.model.in_mut_tx() {
            return match choice {
                InteractionChoice::Replay => Interaction::Replay,

                // Insert/Delete/CommitTx are not legal outside a mutable tx.
                // Treat those weighted choices as pressure to start one.
                InteractionChoice::Insert | InteractionChoice::Delete | InteractionChoice::CommitTx => {
                    Interaction::BeginMutTx
                }
            };
        }

        match choice {
            InteractionChoice::Replay => Interaction::Replay,

            InteractionChoice::Insert => {
                let table = self.insert_table_idx();

                Interaction::Insert {
                    table,
                    row: self.gen_insert_row(table),
                }
            }

            InteractionChoice::Delete => {
                let Some(table) = self.deletable_table_idx() else {
                    return Interaction::CommitTx;
                };

                let row_index = self.rng.index(self.model.row_count(table));

                Interaction::Delete {
                    table,
                    row: self
                        .model
                        .row(table, row_index)
                        .expect("row index is in bounds")
                        .clone(),
                }
            }

            InteractionChoice::CommitTx => Interaction::CommitTx,
        }
    }

    fn pick_interaction_choice(&mut self) -> InteractionChoice {
        let weights = self.weights;

        match self.pick_weighted(&[weights.insert, weights.delete, weights.commit_tx, weights.replay]) {
            0 => InteractionChoice::Insert,
            1 => InteractionChoice::Delete,
            2 => InteractionChoice::CommitTx,
            3 => InteractionChoice::Replay,
            _ => unreachable!(),
        }
    }

    fn pick_weighted(&mut self, weights: &[u64]) -> usize {
        let total: u64 = weights.iter().sum();

        assert!(total > 0, "at least one interaction weight must be non-zero");

        let mut selected = self.rng.next_u64() % total;

        for (idx, weight) in weights.iter().copied().enumerate() {
            if selected < weight {
                return idx;
            }

            selected -= weight;
        }

        unreachable!("selected value is always inside total weight")
    }

    fn insert_table_idx(&self) -> usize {
        let auto_inc_table_idx = self
            .schema()
            .auto_inc_table_and_column()
            .map(|(table_idx, _)| table_idx);

        match auto_inc_table_idx {
            Some(table_idx) if !self.rng.next_u64().is_multiple_of(3) => table_idx,
            _ => self.rng.index(self.schema().tables.len()),
        }
    }

    fn deletable_table_idx(&self) -> Option<usize> {
        self.non_auto_inc_table_idx()
            .filter(|&table_idx| self.model.row_count(table_idx) > 0)
    }
}

impl Debug for WorkloadGen {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:?}", self.stats())
    }
}

impl Iterator for WorkloadGen {
    type Item = Interaction;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_interaction())
    }
}

pub fn row_to_bytes(row: &Row) -> Vec<u8> {
    to_vec(row).expect("row serialization must not fail")
}

pub fn normalize_rows(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by_key(row_to_bytes);
    rows
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountState {
    pub row_counts: Vec<TableRowCount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRowCount {
    pub table: usize,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDelta {
    pub tables: Vec<TableDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDelta {
    pub table: usize,
    pub inserts: Vec<Row>,
    pub deletes: Vec<Row>,
    pub truncated: bool,
}
