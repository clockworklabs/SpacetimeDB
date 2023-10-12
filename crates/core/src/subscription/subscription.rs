use anyhow::Context;
use derive_more::{Deref, DerefMut, From, IntoIterator};
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::{DbTable, MemTable, RelValue};
use spacetimedb_lib::{DataKey, PrimaryKey};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_vm::expr::{self, IndexJoin, JoinExpr, QueryExpr, SourceExpr};
use std::collections::{btree_set, BTreeSet, HashMap, HashSet};
use std::ops::Deref;

use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::error::DBError;
use crate::subscription::query::{run_query, OP_TYPE_FIELD_NAME};
use crate::{
    client::{ClientActorId, ClientConnectionSender},
    db::relational_db::RelationalDB,
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};

use super::query;

pub struct Subscription {
    pub queries: QuerySet,
    pub subscribers: Vec<ClientConnectionSender>,
}

#[derive(Deref, DerefMut, PartialEq, From, IntoIterator)]
pub struct QuerySet(BTreeSet<QueryExpr>);

impl From<QueryExpr> for QuerySet {
    fn from(expr: QueryExpr) -> Self {
        [expr].into()
    }
}

impl<const N: usize> From<[QueryExpr; N]> for QuerySet {
    fn from(exprs: [QueryExpr; N]) -> Self {
        Self(exprs.into())
    }
}

impl FromIterator<QueryExpr> for QuerySet {
    fn from_iter<T: IntoIterator<Item = QueryExpr>>(iter: T) -> Self {
        QuerySet(BTreeSet::from_iter(iter))
    }
}

impl<'a> IntoIterator for &'a QuerySet {
    type Item = &'a QueryExpr;
    type IntoIter = btree_set::Iter<'a, QueryExpr>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Extend<QueryExpr> for QuerySet {
    fn extend<T: IntoIterator<Item = QueryExpr>>(&mut self, iter: T) {
        self.0.extend(iter)
    }
}

impl Subscription {
    pub fn remove_subscriber(&mut self, client_id: ClientActorId) -> Option<ClientConnectionSender> {
        let i = self.subscribers.iter().position(|sub| sub.id == client_id)?;
        Some(self.subscribers.swap_remove(i))
    }

    pub fn add_subscriber(&mut self, sender: ClientConnectionSender) {
        if !self.subscribers.iter().any(|s| s.id == sender.id) {
            self.subscribers.push(sender);
        }
    }
}

// If a RelValue has an id (DataKey) return it directly, otherwise we must construct it from the
// row itself which can be an expensive operation.
fn pk_for_row(row: &RelValue) -> PrimaryKey {
    match row.id {
        Some(data_key) => PrimaryKey { data_key },
        None => RelationalDB::pk_for_row(&row.data),
    }
}

impl QuerySet {
    pub const fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Queries all the [`StTableType::User`] tables *right now*
    /// and turns them into [`QueryExpr`],
    /// the moral equivalent of `SELECT * FROM table`.
    pub(crate) fn get_all(relational_db: &RelationalDB, tx: &MutTxId, auth: &AuthCtx) -> Result<Self, DBError> {
        let tables = relational_db.get_all_tables(tx)?;
        let same_owner = auth.owner == auth.caller;
        let exprs = tables
            .iter()
            .map(Deref::deref)
            .filter(|t| t.table_type == StTableType::User && (same_owner || t.table_access == StAccess::Public))
            .map(QueryExpr::new)
            .collect();

        Ok(Self(exprs))
    }

    /// Incremental evaluation of `rows` that matched the [Query] (aka subscriptions)
    ///
    /// This is equivalent to run a `trigger` on `INSERT/UPDATE/DELETE`, run the [Query] and see if the `row` is matched.
    ///
    /// NOTE: The returned `rows` in [DatabaseUpdate] are **deduplicated** so if 2 queries match the same `row`, only one copy is returned.
    #[tracing::instrument(skip_all)]
    pub fn eval_incr(
        &self,
        relational_db: &RelationalDB,
        tx: &mut MutTxId,
        database_update: &DatabaseUpdate,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut output = DatabaseUpdate { tables: vec![] };
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for expr in self {
            use query::Supported::*;
            match query::classify(expr) {
                None => log::warn!("invalid query expression for incremental evaluation"),
                Some(Scan) => {
                    let source = expr
                        .source
                        .get_db_table()
                        .context("expression without physical source table")?;
                    for table in database_update.tables.iter().filter(|t| t.table_id == source.table_id) {
                        // Get the TableOps for this table
                        let (_, table_row_operations) = table_ops
                            .entry(table.table_id)
                            .or_insert_with(|| (table.table_name.clone(), vec![]));

                        // Replace table reference in original query plan with virtual MemTable
                        let plan = query::to_mem_table(expr.clone(), table);

                        // Evaluate the new plan and capture the new row operations
                        for op in eval_incremental(relational_db, tx, &auth, &plan)?
                            .filter_map(|op| seen.insert((table.table_id, op.row_pk)).then(|| op.into()))
                        {
                            table_row_operations.push(op);
                        }
                    }
                }
                Some(Semijoin) => {
                    if let Some(plan) = IncrementalJoin::new(expr, database_update.tables.iter())? {
                        let table_id = plan.lhs.table.table_id;
                        let header = &plan.lhs.table.head;

                        // Get the TableOps for this table
                        let (_, table_row_operations) = table_ops
                            .entry(table_id)
                            .or_insert_with(|| (header.table_name.clone(), vec![]));

                        // Evaluate the plan and capture the new row operations
                        for op in plan
                            .eval(relational_db, tx, &auth)?
                            .filter_map(|op| seen.insert((table_id, op.row_pk)).then(|| op.into()))
                        {
                            table_row_operations.push(op);
                        }
                    }
                }
            }
        }
        for (table_id, (table_name, ops)) in table_ops.into_iter().filter(|(_, (_, ops))| !ops.is_empty()) {
            output.tables.push(DatabaseTableUpdate {
                table_id,
                table_name,
                ops,
            });
        }
        Ok(output)
    }

    /// Direct execution of [Query] (aka subscriptions)
    ///
    /// This is equivalent to run a direct query like `SELECT * FROM table` and get back all the `rows` that match it.
    ///
    /// NOTE: The returned `rows` in [DatabaseUpdate] are **deduplicated** so if 2 queries match the same `row`, only one copy is returned.
    ///
    /// This is a *major* difference with normal query execution, where is expected to return the full result set for each query.
    #[tracing::instrument(skip_all)]
    pub fn eval(
        &self,
        relational_db: &RelationalDB,
        tx: &mut MutTxId,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut database_update: DatabaseUpdate = DatabaseUpdate { tables: vec![] };
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for q in &self.0 {
            if let Some(t) = q.source.get_db_table() {
                // Get the TableOps for this table
                let (_, table_row_operations) = table_ops
                    .entry(t.table_id)
                    .or_insert_with(|| (t.head.table_name.clone(), vec![]));
                for table in run_query(relational_db, tx, q, auth)? {
                    for row in table.data {
                        let row_pk = pk_for_row(&row);

                        //Skip rows that are already resolved in a previous subscription...
                        if seen.contains(&(t.table_id, row_pk)) {
                            continue;
                        }
                        seen.insert((t.table_id, row_pk));

                        let row_pk = row_pk.to_bytes();
                        let row = row.data;
                        table_row_operations.push(TableOp {
                            op_type: 1, // Insert
                            row_pk,
                            row,
                        });
                    }
                }
            }
        }
        for (table_id, (table_name, ops)) in table_ops.into_iter().filter(|(_, (_, ops))| !ops.is_empty()) {
            database_update.tables.push(DatabaseTableUpdate {
                table_id,
                table_name,
                ops,
            });
        }
        Ok(database_update)
    }
}

/// Helper to retain [`PrimaryKey`] before converting to [`TableOp`].
///
/// [`PrimaryKey`] is [`Copy`], while [`TableOp`] stores it as a [`Vec<u8>`].
struct Op {
    op_type: u8,
    row_pk: PrimaryKey,
    row: ProductValue,
}

impl From<Op> for TableOp {
    fn from(op: Op) -> Self {
        Self {
            op_type: op.op_type,
            row_pk: op.row_pk.to_bytes(),
            row: op.row,
        }
    }
}

fn eval_incremental(
    db: &RelationalDB,
    tx: &mut MutTxId,
    auth: &AuthCtx,
    expr: &QueryExpr,
) -> Result<impl Iterator<Item = Op>, DBError> {
    let results = run_query(db, tx, expr, *auth)?;
    let ops = results
        .into_iter()
        .filter(|result| !result.data.is_empty())
        .flat_map(|result| {
            // Find OP_TYPE_FIELD_NAME injected by [`query::to_mem_table`].
            let pos_op_type = result.head.find_pos_by_name(OP_TYPE_FIELD_NAME).unwrap_or_else(|| {
                panic!(
                    "Failed to locate `{OP_TYPE_FIELD_NAME}` in `{}`, fields: {:?}",
                    result.head.table_name,
                    result.head.fields.iter().map(|x| &x.field).collect::<Vec<_>>()
                )
            });

            result.data.into_iter().map(move |mut row| {
                // Remove the hidden field OP_TYPE_FIELD_NAME, see [`query::to_mem_table`].
                // This must be done before calculating the row PK.
                let op_type = if let AlgebraicValue::U8(op) = row.data.elements.remove(pos_op_type) {
                    op
                } else {
                    panic!(
                        "Failed to extract `{OP_TYPE_FIELD_NAME}` from `{}`",
                        result.head.table_name
                    );
                };
                let row_pk = pk_for_row(&row);
                Op {
                    op_type,
                    row_pk,
                    row: row.data,
                }
            })
        });

    Ok(ops)
}

struct IncrementalJoin<'a> {
    expr: &'a QueryExpr,
    lhs: JoinSide<'a>,
    rhs: JoinSide<'a>,
}

struct JoinSide<'a> {
    table: &'a DbTable,
    updates: DatabaseTableUpdate,
}

impl JoinSide<'_> {
    pub fn inserts(&self) -> DatabaseTableUpdate {
        let ops = self.updates.ops.iter().filter(|op| op.op_type == 1).cloned().collect();
        DatabaseTableUpdate {
            table_id: self.updates.table_id,
            table_name: self.updates.table_name.clone(),
            ops,
        }
    }

    pub fn deletes(&self) -> DatabaseTableUpdate {
        let ops = self.updates.ops.iter().filter(|op| op.op_type == 0).cloned().collect();
        DatabaseTableUpdate {
            table_id: self.updates.table_id,
            table_name: self.updates.table_name.clone(),
            ops,
        }
    }
}

impl<'a> IncrementalJoin<'a> {
    pub fn new(
        expr: &'a QueryExpr,
        updates: impl Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> anyhow::Result<Option<Self>> {
        let mut lhs = expr
            .source
            .get_db_table()
            .map(|table| JoinSide {
                table,
                updates: DatabaseTableUpdate {
                    table_id: table.table_id,
                    table_name: table.head.table_name.clone(),
                    ops: vec![],
                },
            })
            .context("expression without physical source table")?;
        let mut rhs = expr
            .query
            .iter()
            .find_map(|op| match op {
                expr::Query::JoinInner(JoinExpr { rhs, .. })
                | expr::Query::IndexJoin(IndexJoin { probe_side: rhs, .. }) => {
                    rhs.source.get_db_table().map(|table| JoinSide {
                        table,
                        updates: DatabaseTableUpdate {
                            table_id: table.table_id,
                            table_name: table.head.table_name.clone(),
                            ops: vec![],
                        },
                    })
                }
                _ => None,
            })
            .context("rhs table not found")?;

        for update in updates {
            if update.table_id == lhs.table.table_id {
                lhs.updates.ops.extend(update.ops.iter().cloned());
            } else if update.table_id == rhs.table.table_id {
                rhs.updates.ops.extend(update.ops.iter().cloned());
            }
        }

        if lhs.updates.ops.is_empty() && rhs.updates.ops.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self { expr, lhs, rhs }))
        }
    }

    pub fn eval(
        &self,
        db: &RelationalDB,
        tx: &mut MutTxId,
        auth: &AuthCtx,
    ) -> Result<impl Iterator<Item = Op>, DBError> {
        let mut inserts = {
            let lhs_virt = query::to_mem_table(self.expr.clone(), &self.lhs.inserts());
            let rhs_virt = self.to_mem_table_rhs(self.rhs.inserts());

            // {A+ join B}
            let a = eval_incremental(db, tx, auth, &lhs_virt)?;
            // {A join B+}
            let b = run_query(db, tx, &rhs_virt, *auth)?
                .into_iter()
                .filter(|result| !result.data.is_empty())
                .flat_map(|result| {
                    result.data.into_iter().map(move |row| {
                        Op {
                            op_type: 1, // Insert
                            row_pk: pk_for_row(&row),
                            row: row.data,
                        }
                    })
                });
            // {A+ join B+}
            let c = eval_incremental(db, tx, auth, &query::to_mem_table(rhs_virt, &self.lhs.inserts()))?;

            // ({A+ join B} U {A join B+}) \ {A+ join B+ }
            let mut set = a.map(|op| (op.row_pk, op)).collect::<HashMap<PrimaryKey, Op>>();
            set.extend(b.map(|op| (op.row_pk, op)));
            for op in c {
                set.remove(&op.row_pk);
            }

            set
        };
        let mut deletes = {
            let lhs_virt = query::to_mem_table(self.expr.clone(), &self.lhs.deletes());
            let rhs_virt = self.to_mem_table_rhs(self.rhs.deletes());

            // {A- join B}
            let a = eval_incremental(db, tx, auth, &lhs_virt)?;
            // {A join B-}
            let b = run_query(db, tx, &rhs_virt, *auth)?
                .into_iter()
                .filter(|result| !result.data.is_empty())
                .flat_map(|result| {
                    result.data.into_iter().map(move |row| {
                        Op {
                            op_type: 0, // Delete
                            row_pk: pk_for_row(&row),
                            row: row.data,
                        }
                    })
                });
            // {A- join B-}
            let c = eval_incremental(db, tx, auth, &query::to_mem_table(rhs_virt, &self.lhs.deletes()))?;

            // ({A- join B} U {A join B-}) \ {A- join B-}
            let mut set = a.map(|op| (op.row_pk, op)).collect::<HashMap<PrimaryKey, Op>>();
            set.extend(b.map(|op| (op.row_pk, op)));
            for op in c {
                set.remove(&op.row_pk);
            }

            set
        };

        let symmetric_difference = inserts
            .keys()
            .filter(|k| !deletes.contains_key(k))
            .chain(deletes.keys().filter(|k| !inserts.contains_key(k)))
            .copied()
            .collect::<HashSet<PrimaryKey>>();
        inserts.retain(|k, _| symmetric_difference.contains(k));
        deletes.retain(|k, _| symmetric_difference.contains(k));

        // Deletes need to come first, as UPDATE = [DELETE, INSERT]
        Ok(deletes.into_values().chain(inserts.into_values()))
    }

    /// Replace the RHS of the join with a virtual [`MemTable`] of the operations
    /// in [`DatabaseTableUpdate`].
    fn to_mem_table_rhs(&self, updates: DatabaseTableUpdate) -> QueryExpr {
        fn as_rel_value(
            TableOp {
                op_type: _,
                row_pk,
                row,
            }: &TableOp,
        ) -> RelValue {
            let mut bytes: &[u8] = row_pk.as_ref();
            RelValue::new(row.clone(), Some(DataKey::decode(&mut bytes).unwrap()))
        }

        let mut q = self.expr.clone();
        for op in q.query.iter_mut() {
            if let expr::Query::JoinInner(JoinExpr { rhs, .. })
            | expr::Query::IndexJoin(IndexJoin { probe_side: rhs, .. }) = op
            {
                let virt = MemTable::new(
                    self.rhs.table.head.clone(),
                    self.rhs.table.table_access,
                    updates.ops.iter().map(as_rel_value).collect::<Vec<_>>(),
                );
                rhs.source = SourceExpr::MemTable(virt);

                break;
            }
        }

        q
    }
}
