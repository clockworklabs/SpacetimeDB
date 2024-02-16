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
use itertools::Itertools;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_primitives::TableId;
use std::collections::{hash_map, HashMap, HashSet};
use std::ops::Deref;
use std::sync::Arc;

use crate::db::relational_db::Tx;
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::ExecutionContext;
use crate::subscription::query::{run_query, to_mem_table_with_op_type, OP_TYPE_FIELD_NAME};
use crate::{
    client::{ClientActorId, ClientConnectionSender},
    db::relational_db::RelationalDB,
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::PrimaryKey;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::relation::{Header, Relation};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_vm::expr::{self, IndexJoin, QueryExpr};
use spacetimedb_vm::relation::MemTable;

use super::query;

/// A subscription is an [`ExecutionSet`], along with a set of subscribers all
/// interested in the same set of queries.
#[derive(Debug)]
pub struct Subscription {
    pub queries: ExecutionSet,
    subscribers: Vec<ClientConnectionSender>,
}

impl Subscription {
    pub fn new(queries: impl Into<ExecutionSet>, subscriber: ClientConnectionSender) -> Self {
        Self {
            queries: queries.into(),
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

// Returns a closure for evaluating a query.
// One that consists of updates to secondary tables only.
// A secondary table is one whose rows are not directly returned by the query.
// An example is the right table of a left semijoin.
fn evaluator_for_secondary_updates(
    db: &RelationalDB,
    auth: AuthCtx,
    inserts: bool,
) -> impl Fn(&Tx, &QueryExpr) -> Result<Vec<Op>, DBError> + '_ {
    move |tx, query| {
        let ctx = ExecutionContext::incremental_update(db.address());
        let mut out = Vec::new();
        // If we are evaluating inserts, the op type should be 1.
        // Otherwise we are evaluating deletes, and the op type should be 0.
        let op_type = if inserts { 1 } else { 0 };
        for MemTable { data, .. } in run_query(&ctx, db, tx, query, auth)? {
            for row in data {
                let row_pk = RelationalDB::pk_for_row(&row);
                out.push(Op { op_type, row_pk, row });
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
) -> impl Fn(&Tx, &QueryExpr) -> Result<Vec<Op>, DBError> + '_ {
    move |tx, query| {
        let ctx = ExecutionContext::incremental_update(db.address());
        let mut out = Vec::new();
        for MemTable { data, head, .. } in run_query(&ctx, db, tx, query, auth)? {
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
                let op_type = if let AlgebraicValue::U8(op) = row.elements.remove(pos_op_type) {
                    op
                } else {
                    panic!("Failed to extract `{OP_TYPE_FIELD_NAME}` from `{}`", head.table_name);
                };
                let row_pk = RelationalDB::pk_for_row(&row);
                out.push(Op { op_type, row_pk, row });
            }
        }
        Ok(out)
    }
}

/// Helper to retain [`PrimaryKey`] before converting to [`TableOp`].
///
/// [`PrimaryKey`] is [`Copy`], while [`TableOp`] stores it as a [`Vec<u8>`].
#[derive(Debug, PartialEq, Eq, Hash)]
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
    index_side: JoinSide,
    probe_side: JoinSide,
}

/// One side of an [`IncrementalJoin`].
///
/// Holds the "physical" [`DbTable`] this side of the join operates on, as well
/// as the [`DatabaseTableUpdate`]s pertaining that table.
struct JoinSide {
    updates: DatabaseTableUpdate,
}

impl JoinSide {
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
        updates: impl IntoIterator<Item = &'a DatabaseTableUpdate>,
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
    pub fn eval(&self, db: &RelationalDB, tx: &Tx, auth: &AuthCtx) -> Result<impl Iterator<Item = Op>, DBError> {
        let mut inserts = {
            // A query evaluator for inserts
            let eval = |query, is_primary| {
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
            let mut set = HashMap::new();
            set.extend(a.into_iter().map(|op| (op.row_pk, op)));
            set.extend(b.into_iter().map(|op| (op.row_pk, op)));
            set
        };
        let mut deletes = {
            // A query evaluator for deletes
            let eval = |query, is_primary| {
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
            itertools::chain![a, b, c]
                .map(|op| (op.row_pk, op))
                .collect::<HashMap<_, _>>()
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
    fn to_mem_table(head: Arc<Header>, table_access: StAccess, delta: DatabaseTableUpdate) -> MemTable {
        MemTable::new(
            head,
            table_access,
            delta.ops.into_iter().map(|op| op.row).collect::<Vec<_>>(),
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

/// The atomic unit of execution within a subscription set.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExecutionUnit {
    table_id: TableId,
    table_name: String,
    queries: Vec<SupportedQuery>,
}

impl ExecutionUnit {
    fn eval(&self, db: &RelationalDB, tx: &Tx, auth: AuthCtx) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ctx = ExecutionContext::subscribe(db.address());

        let op_type = 1;
        let row_to_op = |row| {
            let row_pk = RelationalDB::pk_for_row(&row);
            Op { op_type, row_pk, row }
        };
        let ops = match &self.queries[..] {
            // special-case single query - we don't have to deduplicate
            [query] => run_query(&ctx, db, tx, &query.expr, auth)?
                .into_iter()
                .flat_map(|table| table.data)
                .map(row_to_op)
                .map_into()
                .collect(),
            queries => {
                let mut ops = Vec::new();
                let mut dup = HashSet::new();

                for SupportedQuery { kind: _, expr } in queries {
                    for table in run_query(&ctx, db, tx, expr, auth)? {
                        let row_ops = table
                            .data
                            .into_iter()
                            .map(row_to_op)
                            .filter(|op| dup.insert(op.row_pk))
                            .map_into();
                        ops.extend(row_ops);
                    }
                }

                ops
            }
        };

        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.table_id,
            table_name: self.table_name.clone(),
            ops,
        }))
    }

    fn eval_incr(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        database_update: &DatabaseUpdate,
        auth: AuthCtx,
    ) -> Result<Option<DatabaseTableUpdate>, DBError> {
        use query::Supported::*;
        let ops = match &self.queries[..] {
            // special-case single query - we don't have to deduplicate
            [query] => {
                match query.kind {
                    Scan => {
                        if let Some(rows) = database_update
                            .tables
                            .iter()
                            .find(|update| update.table_id == self.table_id)
                        {
                            // Replace table reference in original query plan with virtual MemTable
                            let plan = query::to_mem_table(query.expr.clone(), rows);
                            let eval = evaluator_for_primary_updates(db, auth);
                            // Evaluate the new plan and capture the new row operations
                            eval(tx, &plan)?.into_iter().map_into().collect()
                        } else {
                            vec![]
                        }
                    }
                    Semijoin => {
                        if let Some(plan) = IncrementalJoin::new(&query.expr, &database_update.tables)? {
                            // Evaluate the plan and capture the new row operations
                            plan.eval(db, tx, &auth)?.map_into().collect()
                        } else {
                            vec![]
                        }
                    }
                }
            }
            queries => {
                let mut ops = Vec::new();
                let mut dup = HashSet::new();
                let mut is_unique = |op: &Op| dup.insert(op.row_pk);

                for query in queries {
                    match query.kind {
                        Scan => {
                            if let Some(rows) = database_update
                                .tables
                                .iter()
                                .find(|update| update.table_id == self.table_id)
                            {
                                // Replace table reference in original query plan with virtual MemTable
                                let plan = query::to_mem_table(query.expr.clone(), rows);
                                let eval = evaluator_for_primary_updates(db, auth);
                                // Evaluate the new plan and capture the new row operations
                                ops.extend(eval(tx, &plan)?.into_iter().filter(&mut is_unique).map_into());
                            }
                        }
                        Semijoin => {
                            for rows in database_update.tables.iter().filter(|table| {
                                table.table_id == self.table_id || query.expr.reads_from_table(&table.table_id)
                            }) {
                                if let Some(plan) = IncrementalJoin::new(&query.expr, [rows])? {
                                    // Evaluate the plan and capture the new row operations
                                    ops.extend(plan.eval(db, tx, &auth)?.filter(&mut is_unique).map_into());
                                }
                            }
                        }
                    }
                }

                ops
            }
        };

        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.table_id,
            table_name: self.table_name.clone(),
            ops,
        }))
    }
}

/// A set of independent single or multi-query execution units.
#[derive(Debug, PartialEq, Eq)]
pub struct ExecutionSet {
    exec_units: Vec<ExecutionUnit>,
}

impl ExecutionSet {
    pub fn eval(&self, db: &RelationalDB, tx: &Tx, auth: AuthCtx) -> Result<DatabaseUpdate, DBError> {
        // evaluate each of the execution units in this ExecutionSet in parallel
        let span = tracing::Span::current();
        let tables = self
            .exec_units
            // if you need eval to run single-threaded for debugging, change this to .iter()
            .par_iter()
            .filter_map(|unit| {
                let _entered = span.enter();
                unit.eval(db, tx, auth).transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DatabaseUpdate { tables })
    }

    pub fn eval_incr(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        database_update: &DatabaseUpdate,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let tables = self
            .exec_units
            .iter()
            .filter_map(|unit| unit.eval_incr(db, tx, database_update, auth).transpose())
            .collect::<Result<_, _>>()?;
        Ok(DatabaseUpdate { tables })
    }

    pub fn num_queries(&self) -> usize {
        self.exec_units.iter().map(|unit| unit.queries.len()).sum()
    }
}

impl FromIterator<SupportedQuery> for ExecutionSet {
    fn from_iter<T: IntoIterator<Item = SupportedQuery>>(iter: T) -> Self {
        let mut exec_units = Vec::new();
        // a map from the table id of each execution unit to its index in the vector
        let mut exec_units_map = HashMap::new();
        for query in iter {
            let Some(db_table) = query.expr.source.get_db_table() else {
                continue;
            };
            match exec_units_map.entry(db_table.table_id) {
                hash_map::Entry::Vacant(v) => {
                    v.insert(exec_units.len());
                    exec_units.push(ExecutionUnit {
                        table_id: db_table.table_id,
                        table_name: db_table.head.table_name.clone(),
                        queries: vec![query],
                    });
                }
                hash_map::Entry::Occupied(o) => exec_units[*o.get()].queries.push(query),
            }
        }

        for exec_unit in &mut exec_units {
            exec_unit.queries.sort();
        }
        exec_units.sort();

        ExecutionSet { exec_units }
    }
}

impl From<Vec<SupportedQuery>> for ExecutionSet {
    fn from(value: Vec<SupportedQuery>) -> Self {
        ExecutionSet::from_iter(value)
    }
}

#[cfg(test)]
impl TryFrom<QueryExpr> for ExecutionSet {
    type Error = DBError;

    fn try_from(expr: QueryExpr) -> Result<Self, Self::Error> {
        Ok(ExecutionSet::from_iter(vec![SupportedQuery::try_from(expr)?]))
    }
}

/// Queries all the [`StTableType::User`] tables *right now*
/// and turns them into [`QueryExpr`],
/// the moral equivalent of `SELECT * FROM table`.
pub(crate) fn get_all(relational_db: &RelationalDB, tx: &Tx, auth: &AuthCtx) -> Result<Vec<SupportedQuery>, DBError> {
    Ok(relational_db
        .get_all_tables(tx)?
        .iter()
        .map(Deref::deref)
        .filter(|t| {
            t.table_type == StTableType::User && (auth.owner == auth.caller || t.table_access == StAccess::Public)
        })
        .map(|src| SupportedQuery {
            kind: query::Supported::Scan,
            expr: QueryExpr::new(src),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::host::module_host::TableOp;
    use crate::sql::compiler::compile_sql;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::data_key::ToDataKey;
    use spacetimedb_sats::relation::{DbTable, FieldName};
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::expr::{CrudExpr, IndexJoin, Query, SourceExpr};
    use spacetimedb_vm::relation::Table;

    #[test]
    // Compile an index join after replacing the index side with a virtual table.
    // The original index and probe sides should be swapped after introducing the delta table.
    fn compile_incremental_index_join_index_side() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[(0.into(), "b"), (1.into(), "c")];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

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

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[(0.into(), "b"), (1.into(), "c")];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

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
