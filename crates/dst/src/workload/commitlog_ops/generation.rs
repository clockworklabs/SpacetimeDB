//! Composite generator: reuse `table_ops` and interleave lifecycle + chaos ops.

use std::collections::{BTreeSet, VecDeque};

use crate::{
    schema::SchemaPlan,
    seed::{DstRng, DstSeed},
    workload::{
        commitlog_ops::{CommitlogInteraction, CommitlogWorkloadCase},
        table_ops::{self, TableScenario, TableScenarioId},
    },
};

/// Streaming composite interaction source for commitlog-oriented targets.
pub(crate) struct InteractionStream<S> {
    base: table_ops::InteractionStream<S>,
    rng: DstRng,
    num_connections: usize,
    next_slot: u32,
    alive_slots: BTreeSet<u32>,
    pending: VecDeque<CommitlogInteraction>,
}

impl<S: TableScenario> InteractionStream<S> {
    pub fn new(
        seed: DstSeed,
        scenario: S,
        schema: SchemaPlan,
        num_connections: usize,
        target_interactions: usize,
    ) -> Self {
        Self {
            base: table_ops::InteractionStream::new(seed.fork(123), scenario, schema, num_connections, target_interactions),
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

        if self.rng.index(100) < 18 {
            self.pending.push_back(CommitlogInteraction::ChaosSync);
        }

        if self.rng.index(100) < 9 {
            let conn = self.rng.index(self.num_connections);
            let slot = self.next_slot;
            self.next_slot = self.next_slot.saturating_add(1);
            self.alive_slots.insert(slot);
            self.pending.push_back(CommitlogInteraction::CreateDynamicTable { conn, slot });
            return true;
        }

        if !self.alive_slots.is_empty() && self.rng.index(100) < 6 {
            let conn = self.rng.index(self.num_connections);
            let idx = self.rng.index(self.alive_slots.len());
            let slot = *self
                .alive_slots
                .iter()
                .nth(idx)
                .expect("slot index within alive set bounds");
            self.pending.push_back(CommitlogInteraction::MigrateDynamicTable { conn, slot });
        }

        if !self.alive_slots.is_empty() && self.rng.index(100) < 5 {
            let conn = self.rng.index(self.num_connections);
            let idx = self.rng.index(self.alive_slots.len());
            let slot = *self
                .alive_slots
                .iter()
                .nth(idx)
                .expect("slot index within alive set bounds");
            self.alive_slots.remove(&slot);
            self.pending.push_back(CommitlogInteraction::DropDynamicTable { conn, slot });
        }

        true
    }
}

impl<S: TableScenario> Iterator for InteractionStream<S> {
    type Item = CommitlogInteraction;

    fn next(&mut self) -> Option<Self::Item> {
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

pub(crate) fn materialize_case(
    seed: DstSeed,
    scenario: TableScenarioId,
    max_interactions: usize,
) -> CommitlogWorkloadCase {
    let mut connection_rng = seed.fork(121).rng();
    let num_connections = connection_rng.index(3) + 1;
    let mut schema_rng = seed.fork(122).rng();
    let schema = scenario.generate_schema(&mut schema_rng);
    let interactions = InteractionStream::new(seed, scenario, schema.clone(), num_connections, max_interactions)
        .collect::<Vec<_>>();

    CommitlogWorkloadCase {
        seed,
        scenario,
        num_connections,
        schema,
        interactions,
    }
}

#[allow(dead_code)]
pub(crate) fn base_schema(case: &CommitlogWorkloadCase) -> &SchemaPlan {
    &case.schema
}
