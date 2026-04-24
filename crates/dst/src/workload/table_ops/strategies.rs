//! Typed strategies specific to table-style workload generation.

use crate::{
    seed::DstRng,
    workload::strategy::{Index, Strategy, Weighted},
};

/// Choose one connection uniformly.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConnectionChoice {
    pub(crate) connection_count: usize,
}

impl Strategy<usize> for ConnectionChoice {
    fn sample(&self, rng: &mut DstRng) -> usize {
        Index::new(self.connection_count).sample(rng)
    }
}

/// Choose one table uniformly.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TableChoice {
    pub(crate) table_count: usize,
}

impl Strategy<usize> for TableChoice {
    fn sample(&self, rng: &mut DstRng) -> usize {
        Index::new(self.table_count).sample(rng)
    }
}

/// Weighted transaction control action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TxControlAction {
    Begin,
    Commit,
    Rollback,
    None,
}

/// Strategy for begin/commit/rollback control flow.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TxControlChoice {
    pub(crate) begin_pct: usize,
    pub(crate) commit_pct: usize,
    pub(crate) rollback_pct: usize,
}

impl Strategy<TxControlAction> for TxControlChoice {
    fn sample(&self, rng: &mut DstRng) -> TxControlAction {
        let begin = self.begin_pct.min(100);
        let commit = self.commit_pct.min(100);
        let rollback = self.rollback_pct.min(100);
        let reserved = begin.saturating_add(commit).saturating_add(rollback).min(100);
        let none = 100usize.saturating_sub(reserved);

        Weighted::new(vec![
            (begin, TxControlAction::Begin),
            (commit, TxControlAction::Commit),
            (rollback, TxControlAction::Rollback),
            (none, TxControlAction::None),
        ])
        .sample(rng)
    }
}
