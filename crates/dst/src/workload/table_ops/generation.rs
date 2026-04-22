use std::collections::VecDeque;

use crate::{
    schema::{SchemaPlan, SimRow, TablePlan},
    seed::{DstRng, DstSeed},
};

use super::{model::GenerationModel, TableScenario, TableWorkloadInteraction};

/// Streaming planner for table-oriented workloads.
///
/// The stream keeps only generator state plus a small pending queue, so long
/// duration runs do not need to materialize the full interaction list in
/// memory up front.
#[derive(Clone, Debug)]
pub struct InteractionStream<S> {
    rng: DstRng,
    scenario: S,
    model: GenerationModel,
    num_connections: usize,
    target_interactions: usize,
    emitted: usize,
    finalize_conn: usize,
    pending: VecDeque<TableWorkloadInteraction>,
    finished: bool,
}

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

    pub fn maybe_control_tx(&mut self, conn: usize, begin_pct: usize, commit_pct: usize, rollback_pct: usize) -> bool {
        if !self.model.connections[conn].in_tx && self.model.active_writer().is_none() && self.roll_percent(begin_pct) {
            self.model.begin_tx(conn);
            self.pending.push_back(TableWorkloadInteraction::BeginTx { conn });
            return true;
        }

        if self.model.connections[conn].in_tx && self.roll_percent(commit_pct) {
            let followups = self.model.commit(conn);
            self.pending.push_back(TableWorkloadInteraction::CommitTx { conn });
            self.pending.extend(followups);
            return true;
        }

        if self.model.connections[conn].in_tx && self.roll_percent(rollback_pct) {
            let followups = self.model.rollback(conn);
            self.pending.push_back(TableWorkloadInteraction::RollbackTx { conn });
            self.pending.extend(followups);
            return true;
        }

        false
    }

    pub fn visible_rows(&self, conn: usize, table: usize) -> Vec<crate::schema::SimRow> {
        self.model.visible_rows(conn, table)
    }

    pub fn committed_rows(&self, table: usize) -> Vec<SimRow> {
        self.model.committed_rows(table)
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

    pub fn last_inserted_row(&self, conn: usize) -> Option<crate::schema::SimRow> {
        self.model.last_inserted_row(conn)
    }

    pub fn in_tx(&self, conn: usize) -> bool {
        self.model.connections[conn].in_tx
    }

    pub fn table_plan(&self, table: usize) -> &TablePlan {
        &self.model.schema.tables[table]
    }

    pub fn push_interaction(&mut self, interaction: TableWorkloadInteraction) {
        self.pending.push_back(interaction);
    }
}

impl<S: TableScenario> InteractionStream<S> {
    pub fn new(
        seed: DstSeed,
        scenario: S,
        schema: SchemaPlan,
        num_connections: usize,
        target_interactions: usize,
    ) -> Self {
        let scenario_commit_properties = scenario.commit_properties();
        Self {
            rng: seed.fork(17).rng(),
            scenario,
            model: GenerationModel::new(&schema, num_connections, seed, scenario_commit_properties),
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
            while self.finalize_conn < self.num_connections {
                let conn = self.finalize_conn;
                self.finalize_conn += 1;
                if self.model.connections[conn].in_tx {
                    let followups = self.model.commit(conn);
                    self.pending.push_back(TableWorkloadInteraction::CommitTx { conn });
                    self.pending.extend(followups);
                    return;
                }
            }
            self.finished = true;
            return;
        }

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

impl<S: TableScenario> Iterator for InteractionStream<S> {
    type Item = TableWorkloadInteraction;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
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
