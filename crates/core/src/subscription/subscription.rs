//! # Subscription Evaluation
//!
//! This module defines how subscription queries are evaluated.
//!
//! A subscription query returns rows matching one or more SQL SELECT statements
//! alongside information about the affected table and an operation identifier
//! (insert or delete) -- a [`DatabaseUpdate`]. This allows subscribers to
//! maintain their own view of (virtual) tables matching the statements.
//!
//! When the [`Subscription`] is first established, all its queries are
//! evaluated against the database and the results are sent back to the
//! subscriber (see [`QuerySet::eval`]). Afterwards, the [`QuerySet`] is
//! evaluated [incrementally][`QuerySet::eval_incr`] whenever a transaction
//! commits updates to the database.
//!
//! Incremental evaluation is straightforward if a query selects from a single
//! table (`SELECT * FROM table WHERE ...`). For join queries, however, it is
//! not obvious how to compute the minimal set of operations for the client to
//! synchronize its state. In general, we conjecture that server-side
//! materialized views are necessary. We find, however, that a particular kind
//! of join query _can_ be evaluated incrementally without materialized views,
//! as described in the following section:
//!
#![doc = include_str!("../../../../docs/incremental-joins.md")]

use anyhow::Context;
use derive_more::{Deref, DerefMut, From, IntoIterator};
use std::collections::hash_map::DefaultHasher;
use std::collections::{btree_set, BTreeSet, HashMap, HashSet};
use std::hash::Hasher;
use std::ops::Deref;
use std::time::Instant;

use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::db_metrics::{DB_METRICS, MAX_QUERY_CPU_TIME};
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::{ExecutionContext, WorkloadType};
use crate::sql::query_debug_info::QueryDebugInfo;
use crate::subscription::query::{run_query, OP_TYPE_FIELD_NAME};
use crate::{
    client::{ClientActorId, ClientConnectionSender},
    db::relational_db::RelationalDB,
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{Address, PrimaryKey};
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::relation::{Column, DbTable, FieldName, MemTable, RelValue, Relation};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, DataKey, ProductValue};
use spacetimedb_vm::expr::{self, IndexJoin, QueryExpr, SourceExpr};

use super::query;

/// A subscription is a [`QuerySet`], along with a set of subscribers all
/// interested in the same set of queries.
pub struct Subscription {
    pub queries: QuerySet,
    subscribers: Vec<ClientConnectionSender>,
}

impl Subscription {
    pub fn new(queries: QuerySet, subscriber: ClientConnectionSender) -> Self {
        Self {
            queries,
            subscribers: vec![subscriber],
        }
    }

    pub fn subscribers(&self) -> &[ClientConnectionSender] {
        &self.subscribers
    }

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

/// A [`QueryExpr`] tagged with [`query::Supported`].
///
/// Constructed via `TryFrom`, which rejects unsupported queries.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SupportedQuery {
    kind: query::Supported,
    expr: QueryExpr,
    info: QueryDebugInfo,
}

impl SupportedQuery {
    pub fn new(expr: QueryExpr, info: QueryDebugInfo) -> Result<Self, DBError> {
        let kind = query::classify(&expr).ok_or_else(|| SubscriptionError::Unsupported(info.clone()))?;
        Ok(Self { kind, expr, info })
    }

    pub fn kind(&self) -> query::Supported {
        self.kind
    }

    pub fn as_expr(&self) -> &QueryExpr {
        self.as_ref()
    }
}

#[cfg(test)]
impl TryFrom<QueryExpr> for SupportedQuery {
    type Error = DBError;

    fn try_from(expr: QueryExpr) -> Result<Self, Self::Error> {
        let kind = query::classify(&expr).context("Unsupported query expression")?;
        Ok(Self {
            kind,
            expr,
            info: QueryDebugInfo::from_source(""),
        })
    }
}

impl AsRef<QueryExpr> for SupportedQuery {
    fn as_ref(&self) -> &QueryExpr {
        &self.expr
    }
}

/// A set of [supported][`SupportedQuery`] [`QueryExpr`]s.
#[derive(Debug, Deref, DerefMut, PartialEq, From, IntoIterator)]
pub struct QuerySet(BTreeSet<SupportedQuery>);

impl From<SupportedQuery> for QuerySet {
    fn from(q: SupportedQuery) -> Self {
        Self([q].into())
    }
}

impl<const N: usize> From<[SupportedQuery; N]> for QuerySet {
    fn from(qs: [SupportedQuery; N]) -> Self {
        Self(qs.into())
    }
}

impl FromIterator<SupportedQuery> for QuerySet {
    fn from_iter<T: IntoIterator<Item = SupportedQuery>>(iter: T) -> Self {
        QuerySet(BTreeSet::from_iter(iter))
    }
}

impl<'a> IntoIterator for &'a QuerySet {
    type Item = &'a SupportedQuery;
    type IntoIter = btree_set::Iter<'a, SupportedQuery>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Extend<SupportedQuery> for QuerySet {
    fn extend<T: IntoIterator<Item = SupportedQuery>>(&mut self, iter: T) {
        self.0.extend(iter)
    }
}

#[cfg(test)]
impl TryFrom<QueryExpr> for QuerySet {
    type Error = DBError;

    fn try_from(expr: QueryExpr) -> Result<Self, Self::Error> {
        SupportedQuery::try_from(expr).map(Self::from)
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
            .map(|src| SupportedQuery {
                kind: query::Supported::Scan,
                expr: QueryExpr::new(src),
                info: QueryDebugInfo::from_source(format!("SELECT * FROM {}", src.table_name)),
            })
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
        db: &RelationalDB,
        tx: &mut MutTxId,
        database_update: &DatabaseUpdate,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut output = DatabaseUpdate { tables: vec![] };
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for SupportedQuery { kind, expr, info } in self {
            use query::Supported::*;
            let start = Instant::now();
            match kind {
                Scan => {
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
                        for op in eval_incremental(db, tx, &auth, &plan, info)?
                            .filter_map(|op| seen.insert((table.table_id, op.row_pk)).then(|| op.into()))
                        {
                            table_row_operations.push(op);
                        }
                    }
                }
                Semijoin => {
                    if let expr::Query::IndexJoin(ref join) = expr.query[0] {
                        if let Some(plan) = IncrementalJoin::new(join, info, database_update.tables.iter())? {
                            let table = if join.return_index_rows {
                                plan.index_side.table
                            } else {
                                plan.probe_side.table
                            };
                            let table_id = table.table_id;
                            let header = &table.head;

                            // Get the TableOps for this table
                            let (_, table_row_operations) = table_ops
                                .entry(table_id)
                                .or_insert_with(|| (header.table_name.clone(), vec![]));

                            // Evaluate the plan and capture the new row operations
                            for op in plan
                                .eval(db, tx, &auth)?
                                .filter_map(|op| seen.insert((table_id, op.row_pk)).then(|| op.into()))
                            {
                                table_row_operations.push(op);
                            }
                        }
                    }
                }
            }
            record_query_duration_metrics(WorkloadType::Update, &db.address(), info.source(), start);
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
    pub fn eval(&self, db: &RelationalDB, tx: &mut MutTxId, auth: AuthCtx) -> Result<DatabaseUpdate, DBError> {
        let mut database_update: DatabaseUpdate = DatabaseUpdate { tables: vec![] };
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for SupportedQuery { expr, info, .. } in self {
            if let Some(t) = expr.source.get_db_table() {
                let start = Instant::now();
                // Get the TableOps for this table
                let (_, table_row_operations) = table_ops
                    .entry(t.table_id)
                    .or_insert_with(|| (t.head.table_name.clone(), vec![]));
                for table in run_query(
                    &ExecutionContext::subscribe(db.address(), Some(info)),
                    db,
                    tx,
                    expr,
                    auth,
                )? {
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
                record_query_duration_metrics(WorkloadType::Subscribe, &db.address(), info.source(), start);
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

fn record_query_duration_metrics(workload: WorkloadType, db: &Address, query: &str, start: Instant) {
    let query_duration = start.elapsed().as_secs_f64();

    DB_METRICS
        .rdb_query_cpu_time_sec
        .with_label_values(&workload, db, query)
        .observe(query_duration);

    fn hash(a: WorkloadType, b: &Address, c: &str) -> u64 {
        use std::hash::Hash;
        let mut hasher = DefaultHasher::new();
        a.hash(&mut hasher);
        b.hash(&mut hasher);
        c.hash(&mut hasher);
        hasher.finish()
    }

    let max_query_duration = *MAX_QUERY_CPU_TIME
        .lock()
        .unwrap()
        .entry(hash(workload, db, query))
        .and_modify(|max| {
            if query_duration > *max {
                *max = query_duration;
            }
        })
        .or_insert_with(|| query_duration);

    DB_METRICS
        .rdb_query_cpu_time_sec_max
        .with_label_values(&workload, db, query)
        .set(max_query_duration);
}

/// Helper to retain [`PrimaryKey`] before converting to [`TableOp`].
///
/// [`PrimaryKey`] is [`Copy`], while [`TableOp`] stores it as a [`Vec<u8>`].
#[derive(Debug)]
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

/// Incremental evaluation of the supplied [`QueryExpr`].
///
/// The expression is assumed to project a single virtual table consisting
/// of [`DatabaseTableUpdate`]s (see [`query::to_mem_table`]). That is,
/// the `op_type` of the resulting [`Op`]s will be extracted from the virtual
/// column injected by [`query::to_mem_table`], and the virtual column will be
/// removed from the `row`.
fn eval_incremental(
    db: &RelationalDB,
    tx: &mut MutTxId,
    auth: &AuthCtx,
    expr: &QueryExpr,
    info: &QueryDebugInfo,
) -> Result<impl Iterator<Item = Op>, DBError> {
    let ctx = &ExecutionContext::incremental_update(db.address(), Some(info));
    let results = run_query(ctx, db, tx, expr, *auth)?;
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

/// Helper for evaluating a [`query::Supported::Semijoin`].
struct IncrementalJoin<'a> {
    join: &'a IndexJoin,
    info: &'a QueryDebugInfo,
    index_side: JoinSide<'a>,
    probe_side: JoinSide<'a>,
}

/// One side of an [`IncrementalJoin`].
///
/// Holds the "physical" [`DbTable`] this side of the join operates on, as well
/// as the [`DatabaseTableUpdate`]s pertaining that table.
struct JoinSide<'a> {
    table: &'a DbTable,
    updates: DatabaseTableUpdate,
}

impl JoinSide<'_> {
    /// Return a [`DatabaseTableUpdate`] consisting of only insert operations.
    pub fn inserts(&self) -> DatabaseTableUpdate {
        let ops = self.updates.ops.iter().filter(|op| op.op_type == 1).cloned().collect();
        DatabaseTableUpdate {
            table_id: self.updates.table_id,
            table_name: self.updates.table_name.clone(),
            ops,
        }
    }

    /// Return a [`DatabaseTableUpdate`] with only delete operations.
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
    /// Construct an [`IncrementalJoin`] from a [`IndexJoin`] and a series
    /// of [`DatabaseTableUpdate`]s.
    ///
    /// The query expression is assumed to be classified as a
    /// [`query::Supported::Semijoin`] already. The supplied updates are assumed
    /// to be the full set of updates from a single transaction.
    ///
    /// If neither side of the join is modified by any of the updates, `None` is
    /// returned. Otherwise, `Some` [`IncrementalJoin`] is returned with the
    /// updates partitioned into the respective [`JoinSide`].
    ///
    /// An error is returned if the expression is not well-formed.
    pub fn new(
        join: &'a IndexJoin,
        info: &'a QueryDebugInfo,
        updates: impl Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> anyhow::Result<Option<Self>> {
        let mut probe_side = join
            .probe_side
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
        let mut index_side = join
            .index_side
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

        for update in updates {
            if update.table_id == probe_side.table.table_id {
                probe_side.updates.ops.extend(update.ops.iter().cloned());
            } else if update.table_id == index_side.table.table_id {
                index_side.updates.ops.extend(update.ops.iter().cloned());
            }
        }

        if probe_side.updates.ops.is_empty() && index_side.updates.ops.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self {
                join,
                info,
                index_side,
                probe_side,
            }))
        }
    }

    /// Evaluate this [`IncrementalJoin`].
    ///
    /// The following assumptions are made for the incremental evaluation to be
    /// correct without maintaining a materialized view:
    ///
    /// * The join is a primary foreign key semijoin, i.e. one row from the
    ///   right table joins with at most one row from the left table.
    /// * The rows in the [`DatabaseTableUpdate`]s on either side of the join
    ///   are already committed to the underlying "physical" tables.
    /// * We maintain set semantics, i.e. no two rows with the same
    ///   [`PrimaryKey`] can appear in the result.
    ///
    /// Based on this, we evaluate the join as:
    ///
    /// ```text
    ///     let inserts = {A+ join B} U {A join B+}
    ///     let deletes = {A- join B} U {A join B-} U {A- join B-}
    ///
    ///     (deletes \ inserts) || (inserts \ deletes)
    /// ```
    ///
    /// Where:
    ///
    /// * `A`:  Committed table to the LHS of the join.
    /// * `B`:  Committed table to the RHS of the join.
    /// * `+`:  Virtual table of only the insert operations against the annotated table.
    /// * `-`:  Virtual table of only the delete operations against the annotated table.
    /// * `U`:  Set union.
    /// * `\`:  Set difference.
    /// * `||`: Concatenation.
    ///
    /// For a more in-depth discussion, see the [module-level documentation](./index.html).
    pub fn eval(
        &self,
        db: &RelationalDB,
        tx: &mut MutTxId,
        auth: &AuthCtx,
    ) -> Result<impl Iterator<Item = Op>, DBError> {
        let ctx = &ExecutionContext::incremental_update(db.address(), Some(self.info));
        let mut inserts = {
            // Replan query after replacing left table with virtual table,
            // since join order may need to be reversed.
            let index_delta_join = Self::to_mem_index_side(self.join.clone(), self.index_side.inserts());
            let index_delta_expr = Self::to_query_expr(&index_delta_join).optimize(Some(db.address()));
            let probe_delta_join = Self::to_mem_probe_side(self.join.clone(), self.probe_side.inserts());
            let probe_delta_expr = Self::to_query_expr(&probe_delta_join);

            // {A+ join B}
            let a = eval_incremental(db, tx, auth, &index_delta_expr, self.info)?;
            // {A join B+}
            let b = run_query(ctx, db, tx, &probe_delta_expr, *auth)?
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
            // {A+ join B} U {A join B+}
            let mut set = a.map(|op| (op.row_pk, op)).collect::<HashMap<PrimaryKey, Op>>();
            set.extend(b.map(|op| (op.row_pk, op)));
            set
        };
        let mut deletes = {
            // Replan query after replacing left table with virtual table,
            // since join order may need to be reversed.
            let index_delta_join = Self::to_mem_index_side(self.join.clone(), self.index_side.deletes());
            let index_delta_expr = Self::to_query_expr(&index_delta_join).optimize(Some(db.address()));
            let probe_delta_join = Self::to_mem_probe_side(self.join.clone(), self.probe_side.deletes());
            let probe_delta_expr = Self::to_query_expr(&probe_delta_join);

            // {A- join B}
            let a = eval_incremental(db, tx, auth, &index_delta_expr, self.info)?;
            // {A join B-}
            let b = run_query(ctx, db, tx, &probe_delta_expr, *auth)?
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

            let full_delta_join = Self::to_mem_probe_side(index_delta_join, self.index_side.deletes());
            let full_delta_expr = Self::to_query_expr(&full_delta_join);
            // {A- join B-}
            let c = eval_incremental(db, tx, auth, &full_delta_expr, self.info)?;
            // {A- join B} U {A join B-} U {A- join B-}
            let mut set = a.map(|op| (op.row_pk, op)).collect::<HashMap<PrimaryKey, Op>>();
            set.extend(b.map(|op| (op.row_pk, op)));
            set.extend(c.map(|op| (op.row_pk, op)));
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

    fn to_query_expr(join: &IndexJoin) -> QueryExpr {
        let join = join.clone();
        let source: SourceExpr = if join.return_index_rows {
            join.index_side.clone().into()
        } else {
            join.probe_side.source.clone()
        };
        QueryExpr {
            source,
            query: vec![expr::Query::IndexJoin(join)],
        }
    }

    /// Replace the index side of the join with a virtual [`MemTable`] of the operations
    /// in [`DatabaseTableUpdate`].
    fn to_mem_index_side(mut join: IndexJoin, updates: DatabaseTableUpdate) -> IndexJoin {
        let table_access = join.index_side.table_access();
        let head = join.index_side.head();
        let mut t = MemTable::new(head.clone(), table_access, vec![]);

        if let Some(pos) = t.head.find_pos_by_name(OP_TYPE_FIELD_NAME) {
            t.data.extend(updates.ops.iter().map(|row| {
                let mut new = row.row.clone();
                new.elements[pos] = row.op_type.into();
                let mut bytes: &[u8] = row.row_pk.as_ref();
                RelValue::new(new, Some(DataKey::decode(&mut bytes).unwrap()))
            }));
        } else {
            t.head.fields.push(Column::new(
                FieldName::named(&t.head.table_name, OP_TYPE_FIELD_NAME),
                AlgebraicType::U8,
                t.head.fields.len().into(),
            ));
            for row in &updates.ops {
                let mut new = row.row.clone();
                new.elements.push(row.op_type.into());
                let mut bytes: &[u8] = row.row_pk.as_ref();
                t.data
                    .push(RelValue::new(new, Some(DataKey::decode(&mut bytes).unwrap())));
            }
        }

        join.index_side = t.into();
        join
    }

    /// Replace the probe side of the join with a virtual [`MemTable`] of the operations
    /// in [`DatabaseTableUpdate`].
    fn to_mem_probe_side(mut join: IndexJoin, updates: DatabaseTableUpdate) -> IndexJoin {
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

        let virt = MemTable::new(
            join.probe_side.source.head().clone(),
            join.probe_side.source.table_access(),
            updates.ops.iter().map(as_rel_value).collect::<Vec<_>>(),
        );

        join.probe_side.source = SourceExpr::MemTable(virt);
        join
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::host::module_host::TableOp;
    use crate::sql::compiler::compile_sql;
    use itertools::Itertools;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::data_key::ToDataKey;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::*;
    use spacetimedb_sats::relation::Table;
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::expr::{CrudExpr, Query};

    fn create_table(
        db: &RelationalDB,
        tx: &mut MutTxId,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(ColId, &str)],
    ) -> Result<TableId, DBError> {
        let table_name = name.to_string();
        let table_type = StTableType::User;
        let table_access = StAccess::Public;

        let columns = schema
            .iter()
            .map(|(col_name, col_type)| ColumnDef {
                col_name: col_name.to_string(),
                col_type: col_type.clone(),
            })
            .collect_vec();

        let indexes = indexes
            .iter()
            .map(|(col_id, index_name)| IndexDef::btree(index_name.to_string(), *col_id, false))
            .collect_vec();

        let schema = TableDef::new(table_name, columns)
            .with_indexes(indexes)
            .with_type(table_type)
            .with_access(table_access);

        db.create_table(tx, schema)
    }

    #[test]
    fn compile_incremental_index_join_lhs_probe() -> Result<(), DBError> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[(0.into(), "b"), (1.into(), "c")];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, indexes)?;

        // Should generate an index join since there is an index on `rhs.b`.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let Query::IndexJoin(ref join) = expr.query[0] else {
            panic!("unexpected operator {:#?}", expr.query[0]);
        };

        // Create an insert for an incremental update.
        let row = product!(0u64, 0u64, 0u64);
        let insert = TableOp {
            op_type: 1,
            row_pk: row.to_data_key().to_bytes(),
            row,
        };
        let insert = vec![DatabaseTableUpdate {
            table_id: rhs_id,
            table_name: String::from("rhs"),
            ops: vec![insert],
        }];

        // Optimize the query plan for the incremental update.
        let info = QueryDebugInfo::from_source(sql);
        let join = IncrementalJoin::new(join, &info, insert.iter())?.unwrap();
        let join = IncrementalJoin::to_mem_index_side(join.join.clone(), join.index_side.inserts());
        let expr = IncrementalJoin::to_query_expr(&join);
        let expr = expr.optimize(Some(db.address()));

        assert!(expr.source.get_db_table().is_some());
        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let Query::IndexJoin(IndexJoin {
            probe_side:
                QueryExpr {
                    source: SourceExpr::MemTable(_),
                    query: ref rhs,
                },
            probe_field:
                FieldName::Name {
                    table: ref probe_table,
                    field: ref probe_field,
                },
            index_side: Table::DbTable(DbTable {
                table_id: index_table, ..
            }),
            index_select: None,
            index_col,
            return_index_rows: true,
        }) = expr.query[0]
        else {
            panic!("unexpected operator {:#?}", expr.query[0]);
        };

        assert!(!rhs.is_empty());

        // Assert that original index and probe tables have been swapped.
        assert_eq!(index_table, lhs_id);
        assert_eq!(index_col, 1.into());
        assert_eq!(probe_field, "b");
        assert_eq!(probe_table, "rhs");
        Ok(())
    }
}
