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
use std::collections::{btree_set, BTreeSet, HashMap, HashSet};
use std::ops::Deref;
use std::time::Instant;

use crate::db::db_metrics::{DB_METRICS, MAX_QUERY_CPU_TIME};
use crate::db::relational_db::Tx;
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::{ExecutionContext, WorkloadType};
use crate::subscription::query::{run_query, to_mem_table_with_op_type, OP_TYPE_FIELD_NAME};
use crate::{
    client::{ClientActorId, ClientConnectionSender},
    db::relational_db::RelationalDB,
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{Address, PrimaryKey};
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::relation::{DbTable, Header, MemTable, RelValue, Relation};
use spacetimedb_sats::{AlgebraicValue, DataKey, ProductValue};
use spacetimedb_vm::expr::{self, IndexJoin, QueryExpr};

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
}

impl SupportedQuery {
    pub fn new(expr: QueryExpr, text: String) -> Result<Self, DBError> {
        let kind = query::classify(&expr).ok_or(SubscriptionError::Unsupported(text))?;
        Ok(Self { kind, expr })
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
        Ok(Self { kind, expr })
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

// Returns a closure for evaluating a query.
// One that consists of updates to secondary tables only.
// A secondary table is one whose rows are not directly returned by the query.
// An example is the right table of a left semijoin.
fn evaluator_for_secondary_updates(
    db: &RelationalDB,
    auth: AuthCtx,
    inserts: bool,
) -> impl Fn(&mut Tx, &QueryExpr) -> Result<HashMap<PrimaryKey, Op>, DBError> + '_ {
    move |tx, query| {
        let mut out = HashMap::new();
        // If we are evaluating inserts, the op type should be 1.
        // Otherwise we are evaluating deletes, and the op type should be 0.
        let op_type = if inserts { 1 } else { 0 };
        for MemTable { data, .. } in
            run_query(&ExecutionContext::incremental_update(db.address()), db, tx, query, auth)?
        {
            for row in data {
                let row_pk = pk_for_row(&row);
                let row = row.data;
                out.insert(row_pk, Op { op_type, row_pk, row });
            }
        }
        Ok(out)
    }
}

// Returns a closure for evaluating a query.
// One that consists of updates to its primary table.
// The primary table is the one whose rows are returned by the query.
// An example is the left table of a left semijoin.
fn evaluator_for_primary_updates(
    db: &RelationalDB,
    auth: AuthCtx,
) -> impl Fn(&mut Tx, &QueryExpr) -> Result<HashMap<PrimaryKey, Op>, DBError> + '_ {
    move |tx, query| {
        let mut out = HashMap::new();
        for MemTable { data, head, .. } in
            run_query(&ExecutionContext::incremental_update(db.address()), db, tx, query, auth)?
        {
            // Remove the special __op_type field before computing each row's primary key.
            let pos_op_type = head.find_pos_by_name(OP_TYPE_FIELD_NAME).unwrap_or_else(|| {
                panic!(
                    "Failed to locate `{OP_TYPE_FIELD_NAME}` in `{}`, fields: {:?}",
                    head.table_name,
                    head.fields.iter().map(|x| &x.field).collect::<Vec<_>>()
                )
            });
            let pos_op_type = pos_op_type.idx();

            for mut row in data {
                let op_type = if let AlgebraicValue::U8(op) = row.data.elements.remove(pos_op_type) {
                    op
                } else {
                    panic!("Failed to extract `{OP_TYPE_FIELD_NAME}` from `{}`", head.table_name);
                };
                let row_pk = pk_for_row(&row);
                let row = row.data;
                out.insert(row_pk, Op { op_type, row_pk, row });
            }
        }
        Ok(out)
    }
}

impl QuerySet {
    pub const fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Queries all the [`StTableType::User`] tables *right now*
    /// and turns them into [`QueryExpr`],
    /// the moral equivalent of `SELECT * FROM table`.
    pub(crate) fn get_all(relational_db: &RelationalDB, tx: &Tx, auth: &AuthCtx) -> Result<Self, DBError> {
        let tables = relational_db.get_all_tables(tx)?;
        let same_owner = auth.owner == auth.caller;
        let exprs = tables
            .iter()
            .map(Deref::deref)
            .filter(|t| t.table_type == StTableType::User && (same_owner || t.table_access == StAccess::Public))
            .map(|src| SupportedQuery {
                kind: query::Supported::Scan,
                expr: QueryExpr::new(src),
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
        tx: &mut Tx,
        database_update: &DatabaseUpdate,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut output = DatabaseUpdate { tables: vec![] };
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        let eval = evaluator_for_primary_updates(db, auth);
        for SupportedQuery { kind, expr, .. } in self {
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
                        for op in eval(tx, &plan)?
                            .into_iter()
                            .filter_map(|(row_pk, op)| seen.insert((table.table_id, row_pk)).then(|| op.into()))
                        {
                            table_row_operations.push(op);
                        }
                    }
                }
                Semijoin => {
                    if let Some(plan) = IncrementalJoin::new(expr, database_update.tables.iter())? {
                        let table = plan.left_table();
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
            #[cfg(feature = "metrics")]
            record_query_duration_metrics(WorkloadType::Update, &db.address(), start);
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
    pub fn eval(&self, db: &RelationalDB, tx: &mut Tx, auth: AuthCtx) -> Result<DatabaseUpdate, DBError> {
        let mut database_update: DatabaseUpdate = DatabaseUpdate { tables: vec![] };
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for SupportedQuery { expr, .. } in self {
            if let Some(t) = expr.source.get_db_table() {
                let start = Instant::now();
                // Get the TableOps for this table
                let (_, table_row_operations) = table_ops
                    .entry(t.table_id)
                    .or_insert_with(|| (t.head.table_name.clone(), vec![]));
                for table in run_query(&ExecutionContext::subscribe(db.address()), db, tx, expr, auth)? {
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
                #[cfg(feature = "metrics")]
                record_query_duration_metrics(WorkloadType::Subscribe, &db.address(), start);
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

#[cfg(feature = "metrics")]
fn record_query_duration_metrics(workload: WorkloadType, db: &Address, start: Instant) {
    let query_duration = start.elapsed().as_secs_f64();

    DB_METRICS
        .rdb_query_cpu_time_sec
        .with_label_values(&workload, db)
        .observe(query_duration);

    let max_query_duration = *MAX_QUERY_CPU_TIME
        .lock()
        .unwrap()
        .entry((*db, workload))
        .and_modify(|max| {
            if query_duration > *max {
                *max = query_duration;
            }
        })
        .or_insert_with(|| query_duration);

    DB_METRICS
        .rdb_query_cpu_time_sec_max
        .with_label_values(&workload, db)
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

/// Helper for evaluating a [`query::Supported::Semijoin`].
struct IncrementalJoin<'a> {
    join: &'a IndexJoin,
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
    /// Construct an [`IncrementalJoin`] from a [`QueryExpr`] and a series
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
        join: &'a QueryExpr,
        updates: impl Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> anyhow::Result<Option<Self>> {
        if join.query.len() != 1 {
            return Err(anyhow::anyhow!("expected a single index join, but got {:#?}", join));
        }
        let expr::Query::IndexJoin(ref join) = join.query[0] else {
            return Err(anyhow::anyhow!("expected a single index join, but got {:#?}", join));
        };

        let index_table = join
            .index_side
            .get_db_table()
            .context("expected a physical database table")?;
        let probe_table = join
            .probe_side
            .source
            .get_db_table()
            .context("expected a physical database table")?;

        let index_id = index_table.table_id;
        let probe_id = probe_table.table_id;

        let mut index_side_updates = Vec::new();
        let mut probe_side_updates = Vec::new();

        for update in updates {
            if update.table_id == index_id {
                index_side_updates.extend(update.ops.iter().cloned());
            } else if update.table_id == probe_id {
                probe_side_updates.extend(update.ops.iter().cloned());
            }
        }

        if index_side_updates.is_empty() && probe_side_updates.is_empty() {
            return Ok(None);
        }

        let table = index_table;
        let table_id = index_id;
        let table_name = table.head.table_name.clone();
        let ops = index_side_updates;
        let index_side = JoinSide {
            table,
            updates: DatabaseTableUpdate {
                table_id,
                table_name,
                ops,
            },
        };

        let table = probe_table;
        let table_id = probe_id;
        let table_name = table.head.table_name.clone();
        let ops = probe_side_updates;
        let probe_side = JoinSide {
            table,
            updates: DatabaseTableUpdate {
                table_id,
                table_name,
                ops,
            },
        };

        Ok(Some(Self {
            join,
            index_side,
            probe_side,
        }))
    }

    /// The left table is the primary table.
    /// The one from which rows will be returned.
    fn left_table(&self) -> &DbTable {
        if self.join.return_index_rows {
            self.index_side.table
        } else {
            self.probe_side.table
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
    pub fn eval(&self, db: &RelationalDB, tx: &mut Tx, auth: &AuthCtx) -> Result<impl Iterator<Item = Op>, DBError> {
        let mut inserts = {
            // A query evaluator for inserts
            let mut eval = |query, is_primary| {
                if is_primary {
                    evaluator_for_primary_updates(db, *auth)(tx, query)
                } else {
                    evaluator_for_secondary_updates(db, *auth, true)(tx, query)
                }
            };

            // Replan query after replacing the indexed table with a virtual table,
            // since join order may need to be reversed.
            let join_a = with_delta_table(self.join.clone(), true, self.index_side.inserts());
            let join_a = QueryExpr::from(join_a).optimize(&|table_id, table_name| db.row_count(table_id, table_name));

            // No need to replan after replacing the probe side with a virtual table,
            // since no new constraints have been added.
            let join_b = with_delta_table(self.join.clone(), false, self.probe_side.inserts()).into();

            // {A+ join B}
            let a = eval(&join_a, self.join.return_index_rows)?;
            // {A join B+}
            let b = eval(&join_b, !self.join.return_index_rows)?;
            // {A+ join B} U {A join B+}
            let mut set = a;
            set.extend(b);
            set
        };
        let mut deletes = {
            // A query evaluator for deletes
            let mut eval = |query, is_primary| {
                if is_primary {
                    evaluator_for_primary_updates(db, *auth)(tx, query)
                } else {
                    evaluator_for_secondary_updates(db, *auth, false)(tx, query)
                }
            };

            // Replan query after replacing the indexed table with a virtual table,
            // since join order may need to be reversed.
            let join_a = with_delta_table(self.join.clone(), true, self.index_side.deletes());
            let join_a = QueryExpr::from(join_a).optimize(&|table_id, table_name| db.row_count(table_id, table_name));

            // No need to replan after replacing the probe side with a virtual table,
            // since no new constraints have been added.
            let join_b = with_delta_table(self.join.clone(), false, self.probe_side.deletes()).into();

            // No need to replan after replacing both sides with a virtual tables,
            // since there are no indexes available to us.
            // The only valid plan in this case is that of an inner join.
            let join_c = with_delta_table(self.join.clone(), true, self.index_side.deletes());
            let join_c = with_delta_table(join_c, false, self.probe_side.deletes()).into();

            // {A- join B}
            let a = eval(&join_a, self.join.return_index_rows)?;
            // {A join B-}
            let b = eval(&join_b, !self.join.return_index_rows)?;
            // {A- join B-}
            let c = eval(&join_c, true)?;
            // {A- join B} U {A join B-} U {A- join B-}
            let mut set = a;
            set.extend(b);
            set.extend(c);
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
}

/// Replace an [IndexJoin]'s scan or fetch operation with a delta table.
/// A delta table consists purely of updates or changes to the base table.
fn with_delta_table(mut join: IndexJoin, index_side: bool, delta: DatabaseTableUpdate) -> IndexJoin {
    fn as_rel_value(op: &TableOp) -> RelValue {
        let mut bytes: &[u8] = op.row_pk.as_ref();
        RelValue::new(op.row.clone(), Some(DataKey::decode(&mut bytes).unwrap()))
    }

    fn to_mem_table(head: Header, table_access: StAccess, delta: DatabaseTableUpdate) -> MemTable {
        MemTable::new(
            head,
            table_access,
            delta.ops.iter().map(as_rel_value).collect::<Vec<_>>(),
        )
    }

    // We are replacing the indexed table,
    // and the rows of the indexed table are being returned.
    // Therefore we must add a column with the op type.
    if index_side && join.return_index_rows {
        let head = join.index_side.head().clone();
        let table_access = join.index_side.table_access();
        join.index_side = to_mem_table_with_op_type(head, table_access, &delta).into();
        return join;
    }
    // We are replacing the indexed table,
    // but the rows of the indexed table are not being returned.
    // Therefore we do not need to add a column with the op type.
    if index_side && !join.return_index_rows {
        let head = join.index_side.head().clone();
        let table_access = join.index_side.table_access();
        join.index_side = to_mem_table(head, table_access, delta).into();
        return join;
    }
    // We are replacing the probe table,
    // but the rows of the indexed table are being returned.
    // Therefore we do not need to add a column with the op type.
    if !index_side && join.return_index_rows {
        let head = join.probe_side.source.head().clone();
        let table_access = join.probe_side.source.table_access();
        join.probe_side.source = to_mem_table(head, table_access, delta).into();
        return join;
    }
    // We are replacing the probe table,
    // and the rows of the probe table are being returned.
    // Therefore we must add a column with the op type.
    if !index_side && !join.return_index_rows {
        let head = join.probe_side.source.head().clone();
        let table_access = join.probe_side.source.table_access();
        join.probe_side.source = to_mem_table_with_op_type(head, table_access, &delta).into();
        return join;
    }
    join
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::db::relational_db::MutTx;
    use crate::host::module_host::TableOp;
    use crate::sql::compiler::compile_sql;
    use itertools::Itertools;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::data_key::ToDataKey;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::*;
    use spacetimedb_sats::relation::{FieldName, Table};
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::expr::{CrudExpr, IndexJoin, Query, SourceExpr};

    fn create_table(
        db: &RelationalDB,
        tx: &mut MutTx,
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
    // Compile an index join after replacing the index side with a virtual table.
    // The original index and probe sides should be swapped after introducing the delta table.
    fn compile_incremental_index_join_index_side() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

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
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(mut expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let join = expr.query.pop().unwrap();
        let Query::IndexJoin(join) = join else {
            panic!("expected an index join, but got {:#?}", join);
        };

        // Create an insert for an incremental update.
        let row = product!(0u64, 0u64);
        let insert = TableOp {
            op_type: 1,
            row_pk: row.to_data_key().to_bytes(),
            row,
        };
        let delta = DatabaseTableUpdate {
            table_id: lhs_id,
            table_name: String::from("lhs"),
            ops: vec![insert],
        };

        // Optimize the query plan for the incremental update.
        let expr: QueryExpr = with_delta_table(join, true, delta).into();
        let mut expr = expr.optimize(&|_, _| i64::MAX);
        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let join = expr.query.pop().unwrap();
        let Query::IndexJoin(join) = join else {
            panic!("expected an index join, but got {:#?}", join);
        };

        let IndexJoin {
            probe_side:
                QueryExpr {
                    source: SourceExpr::MemTable(_),
                    query: ref lhs,
                },
            probe_field:
                FieldName::Name {
                    table: ref probe_table,
                    field: ref probe_field,
                },
            index_side: Table::DbTable(DbTable {
                table_id: index_table, ..
            }),
            index_select: Some(_),
            index_col,
            return_index_rows: false,
        } = join
        else {
            panic!("unexpected index join {:#?}", join);
        };

        assert!(lhs.is_empty());

        // Assert that original index and probe tables have been swapped.
        assert_eq!(index_table, rhs_id);
        assert_eq!(index_col, 0.into());
        assert_eq!(probe_field, "b");
        assert_eq!(probe_table, "lhs");
        Ok(())
    }

    #[test]
    // Compile an index join after replacing the probe side with a virtual table.
    // The original index and probe sides should remain after introducing the virtual table.
    fn compile_incremental_index_join_probe_side() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

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
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(mut expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let join = expr.query.pop().unwrap();
        let Query::IndexJoin(join) = join else {
            panic!("expected an index join, but got {:#?}", join);
        };

        // Create an insert for an incremental update.
        let row = product!(0u64, 0u64, 0u64);
        let insert = TableOp {
            op_type: 1,
            row_pk: row.to_data_key().to_bytes(),
            row,
        };
        let delta = DatabaseTableUpdate {
            table_id: rhs_id,
            table_name: String::from("rhs"),
            ops: vec![insert],
        };

        // Optimize the query plan for the incremental update.
        let expr: QueryExpr = with_delta_table(join, false, delta).into();
        let mut expr = expr.optimize(&|_, _| i64::MAX);

        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let join = expr.query.pop().unwrap();
        let Query::IndexJoin(join) = join else {
            panic!("expected an index join, but got {:#?}", join);
        };

        let IndexJoin {
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
        } = join
        else {
            panic!("unexpected index join {:#?}", join);
        };

        assert!(!rhs.is_empty());

        // Assert that original index and probe tables have not been swapped.
        assert_eq!(index_table, lhs_id);
        assert_eq!(index_col, 1.into());
        assert_eq!(probe_field, "b");
        assert_eq!(probe_table, "rhs");
        Ok(())
    }
}
