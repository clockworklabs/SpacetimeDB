use std::fmt::{Debug, Error, Formatter};

use super::generation::{pick_weighted, GenCtx};
use super::migrations::Migration;
use super::model::Model;
use crate::schema::{IndexAlgorithm, SchemaPlan};
use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::{AlgebraicType, ProductValue};
use spacetimedb_runtime::sim::Rng;

pub type Row = ProductValue;

#[derive(Debug, Clone)]
pub enum Interaction {
    BeginMutTx,
    Insert { table: usize, row: Row },
    Delete { table: usize, row: Row },
    CommitTx,
    Migrate(Migration),
    Replay,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionCounts {
    pub total: usize,
    pub begin_mut_tx: usize,
    pub insert: usize,
    pub delete: usize,
    pub commit_tx: usize,
    pub migrate: usize,
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
            Interaction::Migrate(_) => self.migrate += 1,
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
    Migrated,
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
    pub migrate: u64,
    pub replay: u64,
}

impl Default for InteractionWeights {
    fn default() -> Self {
        Self {
            insert: 50,
            delete: 20,
            commit_tx: 28,
            migrate: 1,
            replay: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum InteractionChoice {
    Insert,
    Delete,
    CommitTx,
    Migrate,
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

    fn non_sequenced_table_idx(&self) -> Option<usize> {
        (0..self.schema().tables.len()).find(|&table_idx| {
            let table = &self.schema().tables[table_idx];
            !table.is_event && table.sequences.is_empty()
        })
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
                InteractionChoice::Migrate => self
                    .gen_migration()
                    .map(Interaction::Migrate)
                    .unwrap_or(Interaction::Replay),

                // Insert/Delete/CommitTx are not legal outside a mutable tx.
                // Treat those weighted choices as pressure to start one.
                InteractionChoice::Insert | InteractionChoice::Delete | InteractionChoice::CommitTx => {
                    Interaction::BeginMutTx
                }
            };
        }

        match choice {
            InteractionChoice::Replay => Interaction::Replay,

            InteractionChoice::Migrate => Interaction::CommitTx,

            InteractionChoice::Insert => {
                let table = self.insert_table_idx();

                Interaction::Insert {
                    table,
                    row: GenCtx::new(&self.rng, &self.model).gen_insert_row(table),
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

        match pick_weighted(
            &self.rng,
            &[
                weights.insert,
                weights.delete,
                weights.commit_tx,
                weights.migrate,
                weights.replay,
            ],
        ) {
            0 => InteractionChoice::Insert,
            1 => InteractionChoice::Delete,
            2 => InteractionChoice::CommitTx,
            3 => InteractionChoice::Migrate,
            4 => InteractionChoice::Replay,
            _ => unreachable!(),
        }
    }

    fn insert_table_idx(&self) -> usize {
        let sequenced_tables = self.sequenced_table_indices();
        let data_tables = self.data_table_indices();

        if !sequenced_tables.is_empty() && !self.rng.next_u64().is_multiple_of(3) {
            sequenced_tables[self.rng.index(sequenced_tables.len())]
        } else {
            data_tables[self.rng.index(data_tables.len())]
        }
    }

    fn sequenced_table_indices(&self) -> Vec<usize> {
        self.schema()
            .tables
            .iter()
            .enumerate()
            .filter_map(|(table_idx, table)| (!table.is_event && !table.sequences.is_empty()).then_some(table_idx))
            .collect()
    }

    fn data_table_indices(&self) -> Vec<usize> {
        let data_tables: Vec<_> = self
            .schema()
            .tables
            .iter()
            .enumerate()
            .filter_map(|(table_idx, table)| (!table.is_event).then_some(table_idx))
            .collect();
        assert!(
            !data_tables.is_empty(),
            "engine DST schema must include a non-event table"
        );
        data_tables
    }

    fn gen_migration(&self) -> Option<Migration> {
        GenCtx::new(&self.rng, &self.model).gen_migration()
    }

    fn deletable_table_idx(&self) -> Option<usize> {
        self.non_sequenced_table_idx()
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
    pub table_rows: Vec<TableRows>,
    pub schema: SchemaState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRowCount {
    pub table: usize,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRows {
    pub table: usize,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaState {
    pub tables: Vec<TableSchemaState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchemaState {
    pub table: usize,
    pub name: String,
    pub is_public: bool,
    pub is_event: bool,
    pub primary_key: Option<usize>,
    pub columns: Vec<ColumnState>,
    pub indexes: Vec<IndexState>,
    pub unique_constraints: Vec<UniqueConstraintState>,
    pub sequences: Vec<SequenceState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnState {
    pub name: String,
    pub ty: AlgebraicType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexState {
    pub columns: Vec<usize>,
    pub algorithm: IndexAlgorithmState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexAlgorithmState {
    BTree,
    Hash,
    Direct,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UniqueConstraintState {
    pub columns: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SequenceState {
    pub column: usize,
}

pub fn schema_state_for_plan(schema: &SchemaPlan) -> SchemaState {
    SchemaState {
        tables: schema
            .tables
            .iter()
            .enumerate()
            .map(|(table, table_plan)| {
                let mut indexes = table_plan
                    .indexes
                    .iter()
                    .map(|index| IndexState {
                        columns: index.columns.clone(),
                        algorithm: match index.algorithm {
                            IndexAlgorithm::BTree => IndexAlgorithmState::BTree,
                            IndexAlgorithm::Hash => IndexAlgorithmState::Hash,
                        },
                    })
                    .collect::<Vec<_>>();
                indexes.sort();

                let mut unique_constraints = table_plan
                    .unique_constraints
                    .iter()
                    .map(|constraint| UniqueConstraintState {
                        columns: constraint.columns.clone(),
                    })
                    .collect::<Vec<_>>();
                unique_constraints.sort();

                let mut sequences = table_plan
                    .sequences
                    .iter()
                    .map(|sequence| SequenceState {
                        column: sequence.column,
                    })
                    .collect::<Vec<_>>();
                sequences.sort();

                TableSchemaState {
                    table,
                    name: table_plan.name.clone(),
                    is_public: table_plan.is_public,
                    is_event: table_plan.is_event,
                    primary_key: table_plan.primary_key,
                    columns: table_plan
                        .columns
                        .iter()
                        .map(|column| ColumnState {
                            name: column.name.clone(),
                            ty: column.ty.to_algebraic(),
                        })
                        .collect(),
                    indexes,
                    unique_constraints,
                    sequences,
                }
            })
            .collect(),
    }
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
