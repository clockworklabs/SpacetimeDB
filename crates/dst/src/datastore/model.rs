//! Reference model of the datastore: committed rows per table, plus an overlay
//! for the open transaction.
//!
//! The model is updated with the rows the datastore actually returned, so
//! values the datastore chooses (auto-inc ids) are real here too.

use std::collections::{BTreeMap, BTreeSet};

use spacetimedb_lib::AlgebraicValue;

use super::{normalize_rows, CommitDelta, CountState, InsertOutcome, Row, TableDelta, TableRowCount};
use crate::schema::SchemaPlan;

type UniqueKey = Vec<AlgebraicValue>;

#[derive(Debug)]
pub struct Model {
    schema: SchemaPlan,
    tables: Vec<CommittedTable>,
}

#[derive(Debug, Default)]
struct CommittedTable {
    rows: BTreeSet<Row>,
    /// Per unique constraint: key to the committed row holding it.
    unique: Vec<BTreeMap<UniqueKey, Row>>,
}

/// Changes made by the open transaction, applied on commit.
#[derive(Debug)]
pub struct TxOverlay {
    tables: Vec<PendingTable>,
}

#[derive(Debug, Default)]
struct PendingTable {
    /// Rows inserted by this transaction.
    inserts: BTreeSet<Row>,
    /// Committed rows deleted by this transaction.
    deletes: BTreeSet<Row>,
    /// Per unique constraint: key to the inserted row holding it.
    unique: Vec<BTreeMap<UniqueKey, Row>>,
}

impl Model {
    pub fn new(schema: SchemaPlan) -> Self {
        let tables = schema
            .tables
            .iter()
            .map(|table| CommittedTable {
                rows: BTreeSet::new(),
                unique: vec![BTreeMap::new(); table.unique_constraints.len()],
            })
            .collect();
        Self { schema, tables }
    }

    pub fn begin(&self) -> TxOverlay {
        TxOverlay {
            tables: self
                .tables
                .iter()
                .map(|table| PendingTable {
                    unique: vec![BTreeMap::new(); table.unique.len()],
                    ..PendingTable::default()
                })
                .collect(),
        }
    }

    fn unique_key(&self, table: usize, constraint: usize, row: &Row) -> UniqueKey {
        self.schema.tables[table].unique_constraints[constraint]
            .columns
            .iter()
            .map(|&col| row.elements[col].clone())
            .collect()
    }

    fn is_visible(&self, tx: &TxOverlay, table: usize, row: &Row) -> bool {
        let pending = &tx.tables[table];
        pending.inserts.contains(row) || (self.tables[table].rows.contains(row) && !pending.deletes.contains(row))
    }

    fn violates_unique_constraint(&self, tx: &TxOverlay, table: usize, row: &Row) -> bool {
        let committed = &self.tables[table];
        let pending = &tx.tables[table];
        (0..committed.unique.len()).any(|constraint| {
            let key = self.unique_key(table, constraint, row);
            pending.unique[constraint].contains_key(&key)
                || committed.unique[constraint]
                    .get(&key)
                    .is_some_and(|holder| !pending.deletes.contains(holder))
        })
    }

    /// Insert `row` into the open transaction. Inserting a visible row again is
    /// a no-op that still succeeds.
    pub fn insert(&self, tx: &mut TxOverlay, table: usize, row: &Row) -> InsertOutcome {
        if self.is_visible(tx, table, row) {
            return InsertOutcome::Accepted(row.clone());
        }
        if self.violates_unique_constraint(tx, table, row) {
            return InsertOutcome::UniqueConstraintViolation;
        }

        for constraint in 0..self.tables[table].unique.len() {
            let key = self.unique_key(table, constraint, row);
            tx.tables[table].unique[constraint].insert(key, row.clone());
        }
        tx.tables[table].inserts.insert(row.clone());
        InsertOutcome::Accepted(row.clone())
    }

    /// Delete `row` in the open transaction, if it is visible.
    pub fn delete(&self, tx: &mut TxOverlay, table: usize, row: &Row) {
        if tx.tables[table].inserts.remove(row) {
            for constraint in 0..self.tables[table].unique.len() {
                let key = self.unique_key(table, constraint, row);
                tx.tables[table].unique[constraint].remove(&key);
            }
        } else if self.tables[table].rows.contains(row) {
            tx.tables[table].deletes.insert(row.clone());
        }
    }

    /// Apply the transaction and return the delta it committed.
    pub fn commit(&mut self, tx: TxOverlay) -> CommitDelta {
        let mut deltas = Vec::new();

        for (table, pending) in tx.tables.into_iter().enumerate() {
            if pending.inserts.is_empty() && pending.deletes.is_empty() {
                continue;
            }

            let before = &self.tables[table].rows;
            // A delete followed by the same insert leaves the committed set unchanged.
            let inserts = normalize_rows(pending.inserts.difference(before).cloned().collect());
            let deletes = normalize_rows(pending.deletes.difference(&pending.inserts).cloned().collect());
            let after_count = before.len() - pending.deletes.len() + pending.inserts.len();
            let truncated = !before.is_empty() && after_count == 0 && !deletes.is_empty();

            if !inserts.is_empty() || !deletes.is_empty() || truncated {
                deltas.push(TableDelta {
                    table,
                    inserts,
                    deletes,
                    truncated,
                });
            }

            for row in &pending.deletes {
                self.remove_committed(table, row);
            }
            for row in pending.inserts {
                self.add_committed(table, row);
            }
        }

        CommitDelta { tables: deltas }
    }

    fn remove_committed(&mut self, table: usize, row: &Row) {
        for constraint in 0..self.tables[table].unique.len() {
            let key = self.unique_key(table, constraint, row);
            self.tables[table].unique[constraint].remove(&key);
        }
        self.tables[table].rows.remove(row);
    }

    fn add_committed(&mut self, table: usize, row: Row) {
        for constraint in 0..self.tables[table].unique.len() {
            let key = self.unique_key(table, constraint, &row);
            self.tables[table].unique[constraint].insert(key, row.clone());
        }
        self.tables[table].rows.insert(row);
    }

    /// Number of rows the open transaction sees.
    pub fn visible_count(&self, tx: &TxOverlay, table: usize) -> usize {
        let pending = &tx.tables[table];
        self.tables[table].rows.len() - pending.deletes.len() + pending.inserts.len()
    }

    /// The `index`-th row the open transaction sees.
    pub fn visible_row<'a>(&'a self, tx: &'a TxOverlay, table: usize, index: usize) -> Option<&'a Row> {
        let pending = &tx.tables[table];
        self.tables[table]
            .rows
            .iter()
            .filter(|row| !pending.deletes.contains(*row))
            .chain(pending.inserts.iter())
            .nth(index)
    }

    /// Committed row counts, which is what a replay must restore.
    pub fn committed_counts(&self) -> CountState {
        CountState {
            row_counts: self
                .tables
                .iter()
                .enumerate()
                .map(|(table, state)| TableRowCount {
                    table,
                    count: state.rows.len() as u64,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ColumnPlan, IndexAlgorithm, IndexPlan, TablePlan, Type, UniqueConstraintPlan};

    fn schema() -> SchemaPlan {
        SchemaPlan {
            tables: vec![TablePlan {
                name: "items".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: Type::U64,
                    },
                    ColumnPlan {
                        name: "payload".into(),
                        ty: Type::U64,
                    },
                ],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![0],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: vec![],
                is_public: true,
            }],
        }
    }

    fn row(id: u64, payload: u64) -> Row {
        [AlgebraicValue::U64(id), AlgebraicValue::U64(payload)]
            .into_iter()
            .collect()
    }

    fn committed(model: &mut Model, rows: &[Row]) {
        let mut tx = model.begin();
        for r in rows {
            model.insert(&mut tx, 0, r);
        }
        model.commit(tx);
    }

    fn visible(model: &Model, tx: &TxOverlay) -> Vec<Row> {
        (0..model.visible_count(tx, 0))
            .map(|i| model.visible_row(tx, 0, i).unwrap().clone())
            .collect()
    }

    #[test]
    fn insert_is_visible_before_commit_and_dropped_tx_rolls_back() {
        let model = Model::new(schema());
        let mut tx = model.begin();
        model.insert(&mut tx, 0, &row(1, 0));
        assert_eq!(model.visible_count(&tx, 0), 1);
        drop(tx);

        let tx = model.begin();
        assert_eq!(model.visible_count(&tx, 0), 0);
        assert_eq!(model.committed_counts().row_counts[0].count, 0);
    }

    #[test]
    fn delete_is_visible_before_commit() {
        let mut model = Model::new(schema());
        committed(&mut model, &[row(1, 0), row(2, 0)]);

        let mut tx = model.begin();
        model.delete(&mut tx, 0, &row(1, 0));
        assert_eq!(visible(&model, &tx), vec![row(2, 0)]);
    }

    #[test]
    fn unique_key_conflicts_until_holder_is_deleted() {
        let mut model = Model::new(schema());
        committed(&mut model, &[row(1, 0)]);

        let mut tx = model.begin();
        assert_eq!(
            model.insert(&mut tx, 0, &row(1, 9)),
            InsertOutcome::UniqueConstraintViolation
        );
        assert_eq!(
            model.insert(&mut tx, 0, &row(1, 0)),
            InsertOutcome::Accepted(row(1, 0)),
            "inserting an identical visible row succeeds"
        );
        model.delete(&mut tx, 0, &row(1, 0));
        assert_eq!(model.insert(&mut tx, 0, &row(1, 9)), InsertOutcome::Accepted(row(1, 9)));
        assert_eq!(
            model.insert(&mut tx, 0, &row(1, 5)),
            InsertOutcome::UniqueConstraintViolation,
            "pending inserts hold unique keys too"
        );
    }

    #[test]
    fn commit_reports_delta_and_applies_overlay() {
        let mut model = Model::new(schema());
        committed(&mut model, &[row(1, 0), row(2, 0)]);

        let mut tx = model.begin();
        model.delete(&mut tx, 0, &row(1, 0));
        model.insert(&mut tx, 0, &row(3, 0));
        let delta = model.commit(tx);

        assert_eq!(
            delta.tables,
            vec![TableDelta {
                table: 0,
                inserts: vec![row(3, 0)],
                deletes: vec![row(1, 0)],
                truncated: false,
            }]
        );
        let tx = model.begin();
        assert_eq!(visible(&model, &tx), vec![row(2, 0), row(3, 0)]);
    }

    #[test]
    fn delete_then_reinsert_commits_no_change() {
        let mut model = Model::new(schema());
        committed(&mut model, &[row(1, 0)]);

        let mut tx = model.begin();
        model.delete(&mut tx, 0, &row(1, 0));
        model.insert(&mut tx, 0, &row(1, 0));
        assert!(model.commit(tx).tables.is_empty());
        assert_eq!(model.committed_counts().row_counts[0].count, 1);
    }

    #[test]
    fn deleting_every_row_reports_truncation() {
        let mut model = Model::new(schema());
        committed(&mut model, &[row(1, 0)]);

        let mut tx = model.begin();
        model.delete(&mut tx, 0, &row(1, 0));
        let delta = model.commit(tx);
        assert!(delta.tables[0].truncated);
    }
}
