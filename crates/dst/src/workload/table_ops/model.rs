use std::collections::BTreeSet;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    schema::{generate_value_for_type, SchemaPlan, SimRow},
    seed::{DstRng, DstSeed},
};

use super::{followup_properties_after_commit, property_interaction, TableProperty, TableWorkloadInteraction};

/// Generator-side model of committed rows plus per-connection pending writes.
///
/// This model is used only while producing interactions. It lets the planner
/// pick valid deletes, synthesize visibility checks, and enforce the
/// single-writer discipline before the real target executes anything.
#[derive(Clone, Debug)]
pub(crate) struct GenerationModel {
    pub(crate) schema: SchemaPlan,
    pub(crate) connections: Vec<PendingConnection>,
    committed: Vec<Vec<SimRow>>,
    next_ids: Vec<u64>,
    active_writer: Option<usize>,
    scenario_commit_properties: Vec<TableWorkloadInteraction>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingConnection {
    pub(crate) in_tx: bool,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
    last_auto_committed_insert: Option<SimRow>,
}

impl GenerationModel {
    pub(crate) fn new(
        schema: &SchemaPlan,
        num_connections: usize,
        seed: DstSeed,
        scenario_commit_properties: Vec<TableWorkloadInteraction>,
    ) -> Self {
        Self {
            schema: schema.clone(),
            connections: vec![PendingConnection::default(); num_connections],
            committed: vec![Vec::new(); schema.tables.len()],
            next_ids: (0..schema.tables.len())
                .map(|idx| seed.fork(idx as u64 + 100).0)
                .collect(),
            active_writer: None,
            scenario_commit_properties,
        }
    }

    pub(crate) fn make_row(&mut self, rng: &mut DstRng, table: usize) -> SimRow {
        let table_plan = &self.schema.tables[table];
        let id = self.next_ids[table];
        self.next_ids[table] = self.next_ids[table].wrapping_add(1).max(1);
        let mut values = vec![AlgebraicValue::U64(id)];
        for (idx, col) in table_plan.columns.iter().enumerate().skip(1) {
            values.push(generate_value_for_type(rng, &col.ty, idx));
        }
        SimRow { values }
    }

    pub(crate) fn visible_rows(&self, conn: usize, table: usize) -> Vec<SimRow> {
        let mut rows = self.committed[table].clone();
        let pending = &self.connections[conn];
        for (pending_table, row) in &pending.staged_deletes {
            if *pending_table == table {
                rows.retain(|candidate| candidate != row);
            }
        }
        for (pending_table, row) in &pending.staged_inserts {
            if *pending_table == table {
                rows.push(row.clone());
            }
        }
        rows
    }

    pub(crate) fn committed_rows(&self, table: usize) -> Vec<SimRow> {
        self.committed[table].clone()
    }

    pub(crate) fn active_writer(&self) -> Option<usize> {
        self.active_writer
    }

    pub(crate) fn begin_tx(&mut self, conn: usize) {
        assert!(self.active_writer.is_none(), "single writer already active");
        let pending = &mut self.connections[conn];
        assert!(!pending.in_tx, "connection already in transaction");
        pending.in_tx = true;
        self.active_writer = Some(conn);
    }

    pub(crate) fn insert(&mut self, conn: usize, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn];
        if pending.in_tx {
            pending.staged_inserts.push((table, row));
        } else {
            self.committed[table].push(row.clone());
            pending.last_auto_committed_insert = Some(row);
        }
    }

    pub(crate) fn last_inserted_row(&self, conn: usize) -> Option<SimRow> {
        self.connections[conn].last_auto_committed_insert.clone()
    }

    pub(crate) fn delete(&mut self, conn: usize, table: usize, row: SimRow) {
        let pending = &mut self.connections[conn];
        if pending.in_tx {
            pending
                .staged_inserts
                .retain(|(pending_table, candidate)| !(*pending_table == table && *candidate == row));
            pending.staged_deletes.push((table, row));
        } else {
            self.committed[table].retain(|candidate| *candidate != row);
        }
    }

    pub(crate) fn commit(&mut self, conn: usize) -> Vec<TableWorkloadInteraction> {
        let pending = &mut self.connections[conn];
        let inserts = std::mem::take(&mut pending.staged_inserts);
        let deletes = std::mem::take(&mut pending.staged_deletes);
        pending.in_tx = false;
        self.active_writer = None;

        for (table, row) in &deletes {
            self.committed[*table].retain(|candidate| candidate != row);
        }
        for (table, row) in &inserts {
            self.committed[*table].push(row.clone());
        }

        followup_properties_after_commit(self.scenario_commit_properties.clone(), inserts, deletes)
    }

    pub(crate) fn rollback(&mut self, conn: usize) -> Vec<TableWorkloadInteraction> {
        let pending = &mut self.connections[conn];
        let touched_tables = pending
            .staged_inserts
            .iter()
            .chain(pending.staged_deletes.iter())
            .map(|(table, _)| *table)
            .collect::<BTreeSet<_>>();
        pending.staged_inserts.clear();
        pending.staged_deletes.clear();
        pending.in_tx = false;
        self.active_writer = None;
        let mut followups = touched_tables
            .into_iter()
            .map(|table| {
                property_interaction(TableProperty::RowCountFresh {
                    table,
                    expected: self.committed[table].len() as u64,
                })
            })
            .collect::<Vec<_>>();
        followups.extend(self.scenario_commit_properties.clone());
        followups
    }
}

/// Replay model for the expected final committed state of a table workload.
///
/// The shared runner applies every interaction here in parallel with the real
/// target execution, then compares the collected target outcome against this
/// model at the end of the run.
#[derive(Clone, Debug)]
pub struct ExpectedModel {
    committed: Vec<Vec<SimRow>>,
    connections: Vec<ExpectedConnection>,
    active_writer: Option<usize>,
}

#[derive(Clone, Debug, Default)]
struct ExpectedConnection {
    in_tx: bool,
    staged_inserts: Vec<(usize, SimRow)>,
    staged_deletes: Vec<(usize, SimRow)>,
}

impl ExpectedModel {
    pub fn new(table_count: usize, connection_count: usize) -> Self {
        Self {
            committed: vec![Vec::new(); table_count],
            connections: vec![ExpectedConnection::default(); connection_count],
            active_writer: None,
        }
    }

    pub fn apply(&mut self, interaction: &TableWorkloadInteraction) {
        match interaction {
            TableWorkloadInteraction::BeginTx { conn } => {
                assert!(
                    self.active_writer.is_none(),
                    "multiple concurrent writers in expected model"
                );
                self.connections[*conn].in_tx = true;
                self.active_writer = Some(*conn);
            }
            TableWorkloadInteraction::CommitTx { conn } => {
                assert_eq!(self.active_writer, Some(*conn), "commit by non-owner in expected model");
                let state = &mut self.connections[*conn];
                for (table, row) in state.staged_deletes.drain(..) {
                    self.committed[table].retain(|candidate| *candidate != row);
                }
                for (table, row) in state.staged_inserts.drain(..) {
                    self.committed[table].push(row);
                }
                state.in_tx = false;
                self.active_writer = None;
            }
            TableWorkloadInteraction::RollbackTx { conn } => {
                assert_eq!(
                    self.active_writer,
                    Some(*conn),
                    "rollback by non-owner in expected model"
                );
                let state = &mut self.connections[*conn];
                state.staged_inserts.clear();
                state.staged_deletes.clear();
                state.in_tx = false;
                self.active_writer = None;
            }
            TableWorkloadInteraction::Insert { conn, table, row } => {
                let state = &mut self.connections[*conn];
                if state.in_tx {
                    state.staged_inserts.push((*table, row.clone()));
                } else {
                    self.committed[*table].push(row.clone());
                }
            }
            TableWorkloadInteraction::Delete { conn, table, row } => {
                let state = &mut self.connections[*conn];
                if state.in_tx {
                    state
                        .staged_inserts
                        .retain(|(pending_table, candidate)| !(*pending_table == *table && *candidate == *row));
                    state.staged_deletes.push((*table, row.clone()));
                } else {
                    self.committed[*table].retain(|candidate| *candidate != *row);
                }
            }
            TableWorkloadInteraction::Check(_) => {}
        }
    }

    pub fn committed_rows(mut self) -> Vec<Vec<SimRow>> {
        for table_rows in &mut self.committed {
            table_rows.sort_by_key(|row| row.id().unwrap_or_default());
        }
        self.committed
    }
}
