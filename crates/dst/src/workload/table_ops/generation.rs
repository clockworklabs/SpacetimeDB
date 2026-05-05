use std::collections::VecDeque;

use crate::{
    client::SessionId,
    core::WorkloadSource,
    schema::{ColumnPlan, SchemaPlan, TablePlan},
    seed::{DstRng, DstSeed},
    workload::strategy::{Index, Percent, Strategy},
};

use super::{
    model::GenerationModel,
    strategies::{ConnectionChoice, TableChoice, TxControlAction, TxControlChoice},
    TableScenario, TableWorkloadInteraction,
};

/// Streaming planner for table-oriented workloads.
///
/// The stream keeps only generator state plus a small pending queue, so long
/// duration runs do not need to materialize the full interaction list in
/// memory up front.
#[derive(Clone, Debug)]
pub struct TableWorkloadSource<S> {
    // Deterministic source for all planner choices.
    rng: DstRng,
    // Scenario-specific workload policy layered on top of the shared model.
    scenario: S,
    // Generator-side model used to decide what interactions are legal.
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
        Index::new(len).sample(self.rng)
    }

    pub fn choose_table(&mut self) -> usize {
        TableChoice {
            table_count: self.model.schema.tables.len(),
        }
        .sample(self.rng)
    }

    pub fn roll_percent(&mut self, percent: usize) -> bool {
        Percent::new(percent).sample(self.rng)
    }

    pub fn connection_count(&self) -> usize {
        self.model.connections.len()
    }

    pub fn active_writer(&self) -> Option<SessionId> {
        self.model.active_writer()
    }

    pub fn has_read_tx(&self, conn: SessionId) -> bool {
        self.model.has_read_tx(conn)
    }

    pub fn any_read_tx(&self) -> bool {
        self.model.any_read_tx()
    }

    pub fn begin_read_tx(&mut self, conn: SessionId) {
        self.model.begin_read_tx(conn);
    }

    pub fn release_read_tx(&mut self, conn: SessionId) {
        self.model.release_read_tx(conn);
    }

    pub fn begin_tx(&mut self, conn: SessionId) {
        self.model.begin_tx(conn);
    }

    pub fn commit_tx(&mut self, conn: SessionId) {
        self.model.commit(conn);
    }

    pub fn rollback_tx(&mut self, conn: SessionId) {
        self.model.rollback(conn);
    }

    /// Tries to emit one transaction control interaction for `conn`.
    ///
    /// The shared generator owns transaction lifecycle so scenario code can
    /// focus on domain operations like inserts, deletes, and range checks.
    pub fn maybe_control_tx(
        &mut self,
        conn: SessionId,
        begin_pct: usize,
        commit_pct: usize,
        rollback_pct: usize,
    ) -> bool {
        match (TxControlChoice {
            begin_pct,
            commit_pct,
            rollback_pct,
        })
        .sample(self.rng)
        {
            TxControlAction::Begin
                if !self.model.connections[conn.as_index()].in_tx
                    && !self.model.has_read_tx(conn)
                    && self.model.active_writer().is_none() =>
            {
                self.model.begin_tx(conn);
                self.pending.push_back(TableWorkloadInteraction::begin_tx(conn));
                true
            }
            TxControlAction::Commit if self.model.connections[conn.as_index()].in_tx => {
                self.model.commit(conn);
                self.pending.push_back(TableWorkloadInteraction::commit_tx(conn));
                true
            }
            TxControlAction::Rollback if self.model.connections[conn.as_index()].in_tx => {
                self.model.rollback(conn);
                self.pending.push_back(TableWorkloadInteraction::rollback_tx(conn));
                true
            }
            _ => false,
        }
    }

    pub fn visible_rows(&self, conn: SessionId, table: usize) -> Vec<crate::schema::SimRow> {
        self.model.visible_rows(conn, table)
    }

    pub fn table_plan(&self, table: usize) -> &TablePlan {
        &self.model.schema.tables[table]
    }

    pub fn make_row(&mut self, table: usize) -> crate::schema::SimRow {
        self.model.make_row(self.rng, table)
    }

    pub fn insert(&mut self, conn: SessionId, table: usize, row: crate::schema::SimRow) {
        self.model.insert(conn, table, row);
    }

    pub fn batch_insert(&mut self, conn: SessionId, table: usize, rows: &[crate::schema::SimRow]) {
        self.model.batch_insert(conn, table, rows);
    }

    pub fn delete(&mut self, conn: SessionId, table: usize, row: crate::schema::SimRow) {
        self.model.delete(conn, table, row);
    }

    pub fn batch_delete(&mut self, conn: SessionId, table: usize, rows: &[crate::schema::SimRow]) {
        self.model.batch_delete(conn, table, rows);
    }

    pub fn add_column(&mut self, table: usize, column: ColumnPlan, default: spacetimedb_sats::AlgebraicValue) {
        self.model.add_column(table, column, default);
    }

    pub fn add_index(&mut self, table: usize, cols: Vec<u16>) {
        self.model.add_index(table, cols);
    }

    pub fn absent_row(&mut self, conn: SessionId, table: usize) -> crate::schema::SimRow {
        self.model.absent_row(self.rng, conn, table)
    }

    pub fn unique_key_conflict_row(
        &mut self,
        table: usize,
        source: &crate::schema::SimRow,
    ) -> Option<crate::schema::SimRow> {
        self.model.unique_key_conflict_row(self.rng, table, source)
    }

    pub fn push_interaction(&mut self, interaction: TableWorkloadInteraction) {
        self.pending.push_back(interaction);
    }
}

impl<S: TableScenario> TableWorkloadSource<S> {
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

    pub fn has_open_read_tx(&self) -> bool {
        self.model.any_read_tx()
    }

    pub fn has_open_write_tx(&self) -> bool {
        self.model.active_writer().is_some()
    }

    fn fill_pending(&mut self) {
        if self.emitted >= self.target_interactions {
            // Once the workload budget is spent, stop asking the scenario for
            // more work and only flush any open transaction state.
            while self.finalize_conn < self.num_connections {
                let conn = SessionId::from_index(self.finalize_conn);
                self.finalize_conn += 1;
                if self.model.connections[conn.as_index()].in_tx {
                    self.model.commit(conn);
                    self.pending.push_back(TableWorkloadInteraction::commit_tx(conn));
                    return;
                }
                if self.model.has_read_tx(conn) {
                    self.model.release_read_tx(conn);
                    self.pending.push_back(TableWorkloadInteraction::release_read_tx(conn));
                    return;
                }
            }
            self.finished = true;
            return;
        }

        // Locking targets allow only one writer at a time. If a writer is
        // already open, keep driving that same connection until it commits or
        // rolls back. Otherwise pick a fresh connection uniformly.
        let conn = if let Some(active_writer) = self.model.active_writer() {
            active_writer
        } else if let Some(read_conn) = (0..self.num_connections)
            .map(SessionId::from_index)
            .find(|&conn| self.model.has_read_tx(conn))
        {
            // The current RelationalDB target can block when a write transaction
            // starts behind an open read transaction. Keep driving the snapshot
            // holder until it releases; interleaved read/write snapshots should
            // be reintroduced once the target models that lock behavior.
            read_conn
        } else {
            ConnectionChoice {
                connection_count: self.num_connections,
            }
            .sample(&mut self.rng)
        };
        let mut planner = ScenarioPlanner {
            rng: &mut self.rng,
            model: &mut self.model,
            pending: &mut self.pending,
        };
        self.scenario.fill_pending(&mut planner, conn);
    }
}

impl<S: TableScenario> TableWorkloadSource<S> {
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

impl<S: TableScenario> WorkloadSource for TableWorkloadSource<S> {
    type Interaction = TableWorkloadInteraction;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        self.pull_next_interaction()
    }

    fn request_finish(&mut self) {
        Self::request_finish(self);
    }
}

impl<S: TableScenario> Iterator for TableWorkloadSource<S> {
    type Item = TableWorkloadInteraction;

    fn next(&mut self) -> Option<Self::Item> {
        self.pull_next_interaction()
    }
}
