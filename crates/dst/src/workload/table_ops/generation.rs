use std::collections::VecDeque;

use crate::{
    core::NextInteractionSource,
    schema::SchemaPlan,
    seed::{DstRng, DstSeed},
};

use super::{model::GenerationModel, TableScenario, TableWorkloadInteraction};

/// Streaming planner for table-oriented workloads.
///
/// The stream keeps only generator state plus a small pending queue, so long
/// duration runs do not need to materialize the full interaction list in
/// memory up front.
#[derive(Clone, Debug)]
pub struct NextInteractionGenerator<S> {
    // Deterministic source for all planner choices.
    rng: DstRng,
    // Scenario-specific workload policy layered on top of the shared model.
    scenario: S,
    // Generator-side expected state used to decide what interactions are legal.
    model: GenerationModel,
    num_connections: usize,
    // Soft budget for scenario-generated interactions. Finish mode may emit a
    // few extra commit/follow-up interactions to close open transactions.
    target_interactions: usize,
    emitted: usize,
    // When the budget is exhausted, we walk connections in order and commit any
    // still-open transaction so the stream ends in a clean state.
    finalize_conn: usize,
    // Scenario code can enqueue a burst of interactions at once: for example a
    // mutation followed by one or more property checks.
    pending: VecDeque<TableWorkloadInteraction>,
    finished: bool,
}

/// Narrow helper passed to scenario code so scenario-specific planning can
/// inspect the current model and enqueue interactions without owning the whole
/// stream state machine.
pub struct ScenarioPlanner<'a> {
    rng: &'a mut DstRng,
    model: &'a mut GenerationModel,
    pending: &'a mut VecDeque<TableWorkloadInteraction>,
}

impl<'a> ScenarioPlanner<'a> {
    pub fn choose_index(&mut self, len: usize) -> usize {
        self.rng.index(len)
    }

    pub fn choose_table(&mut self) -> usize {
        self.rng.index(self.model.schema.tables.len())
    }

    pub fn roll_percent(&mut self, percent: usize) -> bool {
        self.rng.index(100) < percent
    }

    /// Tries to emit one transaction control interaction for `conn`.
    ///
    /// The shared generator owns transaction lifecycle so scenario code can
    /// focus on domain operations like inserts, deletes, and range checks.
    pub fn maybe_control_tx(&mut self, conn: usize, begin_pct: usize, commit_pct: usize, rollback_pct: usize) -> bool {
        if !self.model.connections[conn].in_tx && self.model.active_writer().is_none() && self.roll_percent(begin_pct) {
            self.model.begin_tx(conn);
            self.pending.push_back(TableWorkloadInteraction::BeginTx { conn });
            return true;
        }

        if self.model.connections[conn].in_tx && self.roll_percent(commit_pct) {
            self.model.commit(conn);
            self.pending.push_back(TableWorkloadInteraction::CommitTx { conn });
            return true;
        }

        if self.model.connections[conn].in_tx && self.roll_percent(rollback_pct) {
            self.model.rollback(conn);
            self.pending.push_back(TableWorkloadInteraction::RollbackTx { conn });
            return true;
        }

        false
    }

    pub fn visible_rows(&self, conn: usize, table: usize) -> Vec<crate::schema::SimRow> {
        self.model.visible_rows(conn, table)
    }

    pub fn make_row(&mut self, table: usize) -> crate::schema::SimRow {
        self.model.make_row(self.rng, table)
    }

    pub fn insert(&mut self, conn: usize, table: usize, row: crate::schema::SimRow) {
        self.model.insert(conn, table, row);
    }

    pub fn delete(&mut self, conn: usize, table: usize, row: crate::schema::SimRow) {
        self.model.delete(conn, table, row);
    }

    pub fn push_interaction(&mut self, interaction: TableWorkloadInteraction) {
        self.pending.push_back(interaction);
    }
}

impl<S: TableScenario> NextInteractionGenerator<S> {
    pub fn new(
        seed: DstSeed,
        scenario: S,
        schema: SchemaPlan,
        num_connections: usize,
        target_interactions: usize,
    ) -> Self {
        Self {
            rng: seed.fork(17).rng(),
            scenario,
            model: GenerationModel::new(&schema, num_connections, seed),
            num_connections,
            target_interactions,
            emitted: 0,
            finalize_conn: 0,
            pending: VecDeque::new(),
            finished: false,
        }
    }

    pub fn request_finish(&mut self) {
        self.target_interactions = self.emitted;
    }

    fn fill_pending(&mut self) {
        if self.emitted >= self.target_interactions {
            // Once the workload budget is spent, stop asking the scenario for
            // more work and only flush any open transaction state.
            while self.finalize_conn < self.num_connections {
                let conn = self.finalize_conn;
                self.finalize_conn += 1;
                if self.model.connections[conn].in_tx {
                    self.model.commit(conn);
                    self.pending.push_back(TableWorkloadInteraction::CommitTx { conn });
                    return;
                }
            }
            self.finished = true;
            return;
        }

        // Locking targets allow only one writer at a time. If a writer is
        // already open, keep driving that same connection until it commits or
        // rolls back. Otherwise pick a fresh connection uniformly.
        let conn = self
            .model
            .active_writer()
            .unwrap_or_else(|| self.rng.index(self.num_connections));
        let mut planner = ScenarioPlanner {
            rng: &mut self.rng,
            model: &mut self.model,
            pending: &mut self.pending,
        };
        self.scenario.fill_pending(&mut planner, conn);
    }
}

impl<S: TableScenario> NextInteractionGenerator<S> {
    pub fn pull_next_interaction(&mut self) -> Option<TableWorkloadInteraction> {
        loop {
            // Scenario planning fills `pending` in bursts, but the iterator
            // surface stays one interaction at a time.
            if let Some(interaction) = self.pending.pop_front() {
                self.emitted += 1;
                return Some(interaction);
            }

            if self.finished {
                return None;
            }

            self.fill_pending();
        }
    }
}

impl<S: TableScenario> NextInteractionSource for NextInteractionGenerator<S> {
    type Interaction = TableWorkloadInteraction;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        self.pull_next_interaction()
    }

    fn request_finish(&mut self) {
        Self::request_finish(self);
    }
}

impl<S: TableScenario> Iterator for NextInteractionGenerator<S> {
    type Item = TableWorkloadInteraction;

    fn next(&mut self) -> Option<Self::Item> {
        self.pull_next_interaction()
    }
}
