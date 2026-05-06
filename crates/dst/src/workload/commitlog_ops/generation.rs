//! Commitlog workload source: table workload plus lifecycle and durability pressure.

use std::collections::{BTreeSet, VecDeque};

use crate::{
    core::WorkloadSource,
    schema::SchemaPlan,
    seed::{DstRng, DstSeed},
    workload::strategy::{Index, Percent, Strategy},
    workload::{
        commitlog_ops::CommitlogInteraction,
        table_ops::{strategies::ConnectionChoice, TableScenario, TableWorkloadSource},
    },
};

/// Generation profile for commitlog-specific interactions layered around table ops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitlogWorkloadProfile {
    pub(crate) close_reopen_pct: usize,
    pub(crate) snapshot_pct: usize,
    pub(crate) create_dynamic_table_pct: usize,
    pub(crate) migrate_after_create_pct: usize,
    pub(crate) migrate_dynamic_table_pct: usize,
    pub(crate) drop_dynamic_table_pct: usize,
}

impl Default for CommitlogWorkloadProfile {
    fn default() -> Self {
        Self {
            close_reopen_pct: 1,
            snapshot_pct: 2,
            create_dynamic_table_pct: 1,
            migrate_after_create_pct: 55,
            migrate_dynamic_table_pct: 6,
            drop_dynamic_table_pct: 5,
        }
    }
}

/// Streaming source for commitlog-oriented targets.
///
/// This composes a base table workload with commitlog lifecycle interactions
/// instead of defining an unrelated workload language.
pub(crate) struct CommitlogWorkloadSource<S> {
    base: TableWorkloadSource<S>,
    profile: CommitlogWorkloadProfile,
    rng: DstRng,
    num_connections: usize,
    next_slot: u32,
    alive_slots: BTreeSet<u32>,
    pending: VecDeque<CommitlogInteraction>,
}

impl<S: TableScenario> CommitlogWorkloadSource<S> {
    pub fn new(
        seed: DstSeed,
        scenario: S,
        schema: SchemaPlan,
        num_connections: usize,
        target_interactions: usize,
    ) -> Self {
        Self::with_profile(
            seed,
            scenario,
            schema,
            num_connections,
            target_interactions,
            CommitlogWorkloadProfile::default(),
        )
    }

    pub fn with_profile(
        seed: DstSeed,
        scenario: S,
        schema: SchemaPlan,
        num_connections: usize,
        target_interactions: usize,
        profile: CommitlogWorkloadProfile,
    ) -> Self {
        Self {
            base: TableWorkloadSource::new(seed.fork(123), scenario, schema, num_connections, target_interactions),
            profile,
            rng: seed.fork(124).rng(),
            num_connections,
            next_slot: 0,
            alive_slots: BTreeSet::new(),
            pending: VecDeque::new(),
        }
    }

    pub fn request_finish(&mut self) {
        self.base.request_finish();
    }

    fn fill_pending(&mut self) -> bool {
        let Some(base_op) = self.base.next() else {
            return false;
        };
        self.pending.push_back(CommitlogInteraction::Table(base_op));

        if self.base.has_open_read_tx() || self.base.has_open_write_tx() {
            return true;
        }

        if Percent::new(self.profile.close_reopen_pct).sample(&mut self.rng) {
            self.pending.push_back(CommitlogInteraction::CloseReopen);
        }

        if Percent::new(self.profile.snapshot_pct).sample(&mut self.rng) {
            self.pending.push_back(CommitlogInteraction::TakeSnapshot);
        }

        if Percent::new(self.profile.create_dynamic_table_pct).sample(&mut self.rng) {
            let conn = ConnectionChoice {
                connection_count: self.num_connections,
            }
            .sample(&mut self.rng);
            let slot = self.next_slot;
            self.next_slot = self.next_slot.saturating_add(1);
            self.alive_slots.insert(slot);
            self.pending
                .push_back(CommitlogInteraction::CreateDynamicTable { conn, slot });
            // Frequently follow a create with migration to stress add-column +
            // copy + subsequent auto-inc allocation paths.
            if Percent::new(self.profile.migrate_after_create_pct).sample(&mut self.rng) {
                self.pending
                    .push_back(CommitlogInteraction::MigrateDynamicTable { conn, slot });
            }
            return true;
        }

        if !self.alive_slots.is_empty() && Percent::new(self.profile.migrate_dynamic_table_pct).sample(&mut self.rng) {
            let conn = ConnectionChoice {
                connection_count: self.num_connections,
            }
            .sample(&mut self.rng);
            let idx = Index::new(self.alive_slots.len()).sample(&mut self.rng);
            let slot = *self
                .alive_slots
                .iter()
                .nth(idx)
                .expect("slot index within alive set bounds");
            self.pending
                .push_back(CommitlogInteraction::MigrateDynamicTable { conn, slot });
        }

        if !self.alive_slots.is_empty() && Percent::new(self.profile.drop_dynamic_table_pct).sample(&mut self.rng) {
            let conn = ConnectionChoice {
                connection_count: self.num_connections,
            }
            .sample(&mut self.rng);
            let idx = Index::new(self.alive_slots.len()).sample(&mut self.rng);
            let slot = *self
                .alive_slots
                .iter()
                .nth(idx)
                .expect("slot index within alive set bounds");
            self.alive_slots.remove(&slot);
            self.pending
                .push_back(CommitlogInteraction::DropDynamicTable { conn, slot });
        }

        true
    }
}

impl<S: TableScenario> CommitlogWorkloadSource<S> {
    pub fn pull_next_interaction(&mut self) -> Option<CommitlogInteraction> {
        loop {
            if let Some(next) = self.pending.pop_front() {
                return Some(next);
            }
            if !self.fill_pending() {
                return None;
            }
        }
    }
}

impl<S: TableScenario> WorkloadSource for CommitlogWorkloadSource<S> {
    type Interaction = CommitlogInteraction;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        self.pull_next_interaction()
    }

    fn request_finish(&mut self) {
        Self::request_finish(self);
    }
}

impl<S: TableScenario> Iterator for CommitlogWorkloadSource<S> {
    type Item = CommitlogInteraction;

    fn next(&mut self) -> Option<Self::Item> {
        self.pull_next_interaction()
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_sats::AlgebraicType;

    use crate::{
        client::SessionId,
        schema::{ColumnPlan, SchemaPlan, TablePlan},
        seed::{DstRng, DstSeed},
        workload::{
            commitlog_ops::CommitlogInteraction,
            table_ops::{ScenarioPlanner, TableOperation, TableScenario, TableWorkloadInteraction},
        },
    };

    use super::{CommitlogWorkloadProfile, CommitlogWorkloadSource};

    #[derive(Clone)]
    struct BeginThenCommitScenario;

    impl TableScenario for BeginThenCommitScenario {
        fn generate_schema(&self, _rng: &mut DstRng) -> SchemaPlan {
            SchemaPlan {
                tables: vec![TablePlan {
                    name: "test_table".to_string(),
                    columns: vec![ColumnPlan {
                        name: "id".to_string(),
                        ty: AlgebraicType::U64,
                    }],
                    extra_indexes: vec![],
                }],
            }
        }

        fn validate_outcome(
            &self,
            _schema: &SchemaPlan,
            _outcome: &crate::workload::table_ops::TableWorkloadOutcome,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: SessionId) {
            if planner.active_writer() == Some(conn) {
                planner.commit_tx(conn);
                planner.push_interaction(TableWorkloadInteraction::commit_tx(conn));
            } else {
                planner.begin_tx(conn);
                planner.push_interaction(TableWorkloadInteraction::begin_tx(conn));
            }
        }
    }

    #[test]
    fn lifecycle_interactions_wait_for_open_write_tx_to_close() {
        let scenario = BeginThenCommitScenario;
        let mut rng = DstSeed(1).rng();
        let schema = scenario.generate_schema(&mut rng);
        let profile = CommitlogWorkloadProfile {
            close_reopen_pct: 100,
            snapshot_pct: 100,
            create_dynamic_table_pct: 100,
            migrate_after_create_pct: 100,
            migrate_dynamic_table_pct: 100,
            drop_dynamic_table_pct: 100,
        };
        let mut source = CommitlogWorkloadSource::with_profile(DstSeed(10), scenario, schema, 1, 2, profile);

        assert!(matches!(
            source.next(),
            Some(CommitlogInteraction::Table(TableWorkloadInteraction {
                op: TableOperation::BeginTx { .. },
                ..
            }))
        ));
        assert!(matches!(
            source.next(),
            Some(CommitlogInteraction::Table(TableWorkloadInteraction {
                op: TableOperation::CommitTx { .. },
                ..
            }))
        ));
        assert!(matches!(source.next(), Some(CommitlogInteraction::CloseReopen)));
    }
}
