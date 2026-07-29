//! Workload interaction generation for the engine DST driver.

use std::fmt::{Debug, Error as FmtError, Formatter};

use super::generation::{GenCtx, GenerationState};
use super::migrations::{Migration, MigrationRejection};
use super::model::Model;
use super::row::Row;
use super::state::{CommitDelta, CountState};
use crate::rng::{choice, pick_choice, Choice};
use crate::schema::SchemaPlan;
use crate::traits::InteractionGen;
use anyhow::Error;
use spacetimedb_runtime::sim::Rng;

/// One generated action for the engine target to execute.
#[derive(Debug, Clone)]
pub enum Interaction {
    BeginMutTx,
    Insert { table: usize, row: Row },
    Delete { table: usize, row: Row },
    CommitTx,
    Migrate(Migration),
    Replay,
}

/// Counts of emitted workload interactions, reported at the end of each run.
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

/// Observable result of executing an interaction against the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    BeganMutTx,
    Inserted { outcome: InsertOutcome },
    Deleted,
    Committed { delta: CommitDelta },
    Migrated,
    MigrationRejected { reason: MigrationRejection },
    Replayed { state: CountState },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Accepted(Row),
    UniqueConstraintViolation { details: String },
}

/// Runtime-tunable weights for top-level workload actions.
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
            insert: 5_000,
            delete: 2_000,
            commit_tx: 2_800,
            migrate: 100,
            replay: 1,
        }
    }
}

/// Stateful interaction source that mirrors target observations into the model.
pub struct WorkloadGen {
    rng: Rng,
    model: Model,
    generation: GenerationState,
    stats: InteractionCounts,
    weights: InteractionWeights,
}

impl WorkloadGen {
    pub fn new(rng: Rng, model: Model) -> Self {
        Self::with_weights(rng, model, InteractionWeights::default())
    }

    pub fn with_weights(rng: Rng, model: Model, weights: InteractionWeights) -> Self {
        let generation = GenerationState::seeded(model.schema());
        Self {
            rng,
            model,
            generation,
            stats: InteractionCounts::default(),
            weights,
        }
    }

    pub fn stats(&self) -> InteractionCounts {
        self.stats
    }

    pub fn next_interaction(&mut self) -> Interaction {
        let choice = self.pick_interaction_choice();
        let interaction = self.interaction_from_choice(choice);

        self.stats.record(&interaction);

        interaction
    }

    fn observe_interaction(&mut self, interaction: &Interaction, observation: &Observation) -> Result<(), Error> {
        let model_observation = match (interaction, observation) {
            (Interaction::Insert { .. }, Observation::Inserted { .. }) => self.model.apply(interaction),
            (Interaction::Insert { .. }, _) => anyhow::bail!("insert produced unexpected observation"),
            _ => self.model.apply(interaction),
        };

        self.observe_generation(interaction, &model_observation);
        Ok(())
    }

    fn observe_generation(&mut self, interaction: &Interaction, model_observation: &Observation) {
        match (interaction, model_observation) {
            (Interaction::Insert { table, row }, Observation::Inserted { outcome }) => {
                let table_plan = self.model.schema().tables[*table].clone();
                self.generation.observe_row(&table_plan, row);
                if let InsertOutcome::Accepted(row) = outcome {
                    self.generation.observe_row(&table_plan, row);
                }
            }
            (_, Observation::Migrated) => self.observe_schema_values(),
            _ => {}
        }
    }

    fn observe_schema_values(&mut self) {
        let schema = self.model.schema().clone();
        self.generation.seed_schema(&schema);
        for (table, table_plan) in schema.tables.iter().enumerate() {
            let rows = (0..self.model.row_count(table))
                .map(|row| self.model.row(table, row).expect("row index is in bounds").clone())
                .collect::<Vec<_>>();
            for row in rows {
                self.generation.observe_row(table_plan, &row);
            }
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

impl InteractionWeights {
    fn choices(self) -> [Choice<InteractionChoice>; 5] {
        [
            choice(self.insert, InteractionChoice::Insert),
            choice(self.delete, InteractionChoice::Delete),
            choice(self.commit_tx, InteractionChoice::CommitTx),
            choice(self.migrate, InteractionChoice::Migrate),
            choice(self.replay, InteractionChoice::Replay),
        ]
    }
}

impl WorkloadGen {
    fn schema(&self) -> &SchemaPlan {
        self.model.schema()
    }

    fn non_sequenced_table_idx(&self) -> Option<usize> {
        (0..self.schema().tables.len()).find(|&table_idx| {
            let table = &self.schema().tables[table_idx];
            !table.is_event && table.sequences.is_empty()
        })
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

                let mut ctx = GenCtx::new(&self.rng, &self.model, &mut self.generation);
                Interaction::Insert {
                    table,
                    row: ctx.gen_insert_row(table),
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
        let choices = self.weights.choices();
        pick_choice(&self.rng, &choices)
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

    fn gen_migration(&mut self) -> Option<Migration> {
        GenCtx::new(&self.rng, &self.model, &mut self.generation).gen_migration()
    }

    fn deletable_table_idx(&self) -> Option<usize> {
        self.non_sequenced_table_idx()
            .filter(|&table_idx| self.model.row_count(table_idx) > 0)
    }
}

impl Debug for WorkloadGen {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(f, "{:?}", self.stats())
    }
}

impl InteractionGen for WorkloadGen {
    type Interaction = Interaction;
    type Observation = Observation;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        Some(WorkloadGen::next_interaction(self))
    }

    fn observe(&mut self, interaction: &Self::Interaction, observation: &Self::Observation) -> Result<(), Error> {
        self.observe_interaction(interaction, observation)
    }
}
