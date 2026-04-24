//! Composite generator: reuse `table_ops` and interleave lifecycle + chaos ops.

use std::collections::{BTreeSet, VecDeque};

use crate::{
    core::NextInteractionSource,
    schema::SchemaPlan,
    seed::{DstRng, DstSeed},
    workload::{
        commitlog_ops::CommitlogInteraction,
        table_ops::{NextInteractionGenerator, TableScenario},
    },
};

/// Streaming composite interaction source for commitlog-oriented targets.
pub(crate) struct NextInteractionGeneratorComposite<S> {
    base: NextInteractionGenerator<S>,
    rng: DstRng,
    num_connections: usize,
    next_slot: u32,
    alive_slots: BTreeSet<u32>,
    pending: VecDeque<CommitlogInteraction>,
}

impl<S: TableScenario> NextInteractionGeneratorComposite<S> {
    pub fn new(
        seed: DstSeed,
        scenario: S,
        schema: SchemaPlan,
        num_connections: usize,
        target_interactions: usize,
    ) -> Self {
        Self {
            base: NextInteractionGenerator::new(seed.fork(123), scenario, schema, num_connections, target_interactions),
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
        if self.rng.index(100) < 4 {
            self.pending.push_back(CommitlogInteraction::CloseReopen);
        }

        if self.rng.index(100) < 9 {
            let conn = self.rng.index(self.num_connections);
            let slot = self.next_slot;
            self.next_slot = self.next_slot.saturating_add(1);
            self.alive_slots.insert(slot);
            self.pending
                .push_back(CommitlogInteraction::CreateDynamicTable { conn, slot });
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
            self.pending
                .push_back(CommitlogInteraction::MigrateDynamicTable { conn, slot });
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
            self.pending
                .push_back(CommitlogInteraction::DropDynamicTable { conn, slot });
        }

        true
    }
}

impl<S: TableScenario> NextInteractionGeneratorComposite<S> {
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

impl<S: TableScenario> NextInteractionSource for NextInteractionGeneratorComposite<S> {
    type Interaction = CommitlogInteraction;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        self.pull_next_interaction()
    }

    fn request_finish(&mut self) {
        Self::request_finish(self);
    }
}

impl<S: TableScenario> Iterator for NextInteractionGeneratorComposite<S> {
    type Item = CommitlogInteraction;

    fn next(&mut self) -> Option<Self::Item> {
        self.pull_next_interaction()
    }
}
