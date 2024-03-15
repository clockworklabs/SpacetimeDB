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

use super::execution_unit::ExecutionUnit;
use super::query;
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp};
use crate::vm::{build_query, TxMode};
use anyhow::Context;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::ProductValue;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::relation::{DbTable, Header};
use spacetimedb_vm::expr::{self, IndexJoin, Query, QueryExpr, SourceSet};
use spacetimedb_vm::rel_ops::RelOps;
use spacetimedb_vm::relation::MemTable;
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;

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
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SupportedQuery {
    pub kind: query::Supported,
    pub expr: QueryExpr,
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

    /// The table whose rows are being returned.
    pub fn return_table(&self) -> TableId {
        self.expr.source.get_db_table().unwrap().table_id
    }

    pub fn return_name(&self) -> String {
        self.expr.source.table_name().to_owned()
    }

    /// This is the same as the return table unless this is a join.
    /// For joins this is the table whose rows are not being returned.
    pub fn filter_table(&self) -> TableId {
        let return_table = self.return_table();
        self.expr
            .query
            .first()
            .and_then(|op| {
                if let Query::IndexJoin(join) = op {
                    Some(join)
                } else {
                    None
                }
            })
            .and_then(|join| {
                join.index_side
                    .get_db_table()
                    .filter(|t| t.table_id != return_table)
                    .or_else(|| join.probe_side.source.get_db_table())
                    .filter(|t| t.table_id != return_table)
                    .map(|t| t.table_id)
            })
            .unwrap_or(return_table)
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

/// Evaluates `query` and returns all the updates.
fn eval_updates(
    db: &RelationalDB,
    tx: &Tx,
    query: &QueryExpr,
    mut sources: SourceSet,
) -> Result<impl Iterator<Item = ProductValue>, DBError> {
    let ctx = ExecutionContext::incremental_update(db.address());
    let tx: TxMode = tx.into();
    // TODO(perf, 833): avoid clone.
    let query = build_query(&ctx, db, &tx, query.clone(), &mut sources)?;
    // TODO(perf): avoid collecting into a vec.
    Ok(query.collect_vec(|row_ref| row_ref.into_product_value())?.into_iter())
}

/// A [`query::Supported::Semijoin`] compiled for incremental evaluations.
///
/// The following assumptions are made for the incremental evaluation to be
/// correct without maintaining a materialized view:
///
/// * The join is a primary foreign key semijoin, i.e. one row from the
///   right table joins with at most one row from the left table.
/// * The rows in the [`DatabaseTableUpdate`]s on either side of the join
///   are already committed to the underlying "physical" tables.
/// * We maintain set semantics, i.e. no two rows with the same value can appear in the result.
///
/// See [IncrementalJoin::eval] for a detailed algorithmic explanation.
/// However note that there are at most three distinct plans that we must evaluate.
/// They are:
///
/// 1. A(+|-) x B
/// 2. A x B(+|-)
/// 3. A(+|-) x B(+|-)
///
/// All three of these plans are compiled ahead of time,
/// before the evaluation of any row updates.
///
/// For a more in-depth discussion, see the [module-level documentation](./index.html).
#[derive(Debug)]
pub struct IncrementalJoin {
    /// The lhs table which may be the index side or the probe side.
    lhs: DbTable,
    /// The rhs table which may be the index side or the probe side.
    rhs: DbTable,
    /// This determines which side is the index side and which is the probe side.
    return_index_rows: bool,
    /// A(+|-) join B
    virtual_index_plan: QueryExpr,
    /// A join B(+|-)
    virtual_probe_plan: QueryExpr,
    /// A(+|-) join B(+|-)
    virtual_plan: QueryExpr,
}

/// One side of an [`IncrementalJoin`].
///
/// Holds the "physical" [`DbTable`] this side of the join operates on, as well
/// as the [`DatabaseTableUpdate`]s pertaining that table.
struct JoinSide {
    table_id: TableId,
    table_name: String,
    inserts: Vec<TableOp>,
    deletes: Vec<TableOp>,
}

impl JoinSide {
    /// Return a [`DatabaseTableUpdate`] consisting of only insert operations.
    pub fn inserts(&self) -> DatabaseTableUpdate {
        DatabaseTableUpdate {
            table_id: self.table_id,
            table_name: self.table_name.clone(),
            ops: self.inserts.to_vec(),
        }
    }

    /// Return a [`DatabaseTableUpdate`] with only delete operations.
    pub fn deletes(&self) -> DatabaseTableUpdate {
        DatabaseTableUpdate {
            table_id: self.table_id,
            table_name: self.table_name.clone(),
            ops: self.deletes.to_vec(),
        }
    }

    /// Does this table update include inserts?
    pub fn has_inserts(&self) -> bool {
        !self.inserts.is_empty()
    }

    /// Does this table update include deletes?
    pub fn has_deletes(&self) -> bool {
        !self.deletes.is_empty()
    }
}

impl IncrementalJoin {
    /// Construct an empty [`DatabaseTableUpdate`] with the schema of `table`
    /// to use as a source when pre-compiling `eval_incr` queries.
    fn dummy_table_update(table: &DbTable) -> DatabaseTableUpdate {
        let table_id = table.table_id;
        let table_name = table.head.table_name.clone();
        DatabaseTableUpdate {
            table_id,
            table_name,
            ops: vec![],
        }
    }

    fn optimize_query(join: IndexJoin) -> QueryExpr {
        let expr = QueryExpr::from(join);
        // Because (at least) one of the two tables will be a `MemTable`,
        // and therefore not have indexes,
        // the `row_count` function we pass to `optimize` is useless;
        // either the `DbTable` must be used as the index side,
        // or for the `A- join B-` case, the join must be rewritten to not use indexes.
        expr.optimize(&|_, _| 0)
    }

    /// Return the query plan where the lhs is a delta table.
    fn plan_for_delta_lhs(&self) -> &QueryExpr {
        if self.return_index_rows {
            &self.virtual_index_plan
        } else {
            &self.virtual_probe_plan
        }
    }

    /// Return the query plan where the rhs is a delta table.
    fn plan_for_delta_rhs(&self) -> &QueryExpr {
        if self.return_index_rows {
            &self.virtual_probe_plan
        } else {
            &self.virtual_index_plan
        }
    }

    /// Construct an [`IncrementalJoin`] from a [`QueryExpr`].
    ///
    ///
    /// An error is returned if the expression is not well-formed.
    pub fn new(expr: &QueryExpr) -> anyhow::Result<Self> {
        if expr.query.len() != 1 {
            return Err(anyhow::anyhow!("expected a single index join, but got {:#?}", expr));
        }
        let expr::Query::IndexJoin(ref join) = expr.query[0] else {
            return Err(anyhow::anyhow!("expected a single index join, but got {:#?}", expr));
        };

        let index_table = join
            .index_side
            .get_db_table()
            .context("expected a physical database table")?
            .clone();
        let probe_table = join
            .probe_side
            .source
            .get_db_table()
            .context("expected a physical database table")?
            .clone();

        let (virtual_index_plan, _sources) =
            with_delta_table(join.clone(), Some(Self::dummy_table_update(&index_table)), None);
        debug_assert_eq!(_sources.len(), 1);
        let virtual_index_plan = Self::optimize_query(virtual_index_plan);

        let (virtual_probe_plan, _sources) =
            with_delta_table(join.clone(), None, Some(Self::dummy_table_update(&probe_table)));
        debug_assert_eq!(_sources.len(), 1);
        let virtual_probe_plan = Self::optimize_query(virtual_probe_plan);

        let (virtual_plan, _sources) = with_delta_table(
            join.clone(),
            Some(Self::dummy_table_update(&index_table)),
            Some(Self::dummy_table_update(&probe_table)),
        );
        debug_assert_eq!(_sources.len(), 2);
        let virtual_plan = virtual_plan.to_inner_join();

        let return_index_rows = join.return_index_rows;

        let (lhs, rhs) = if return_index_rows {
            (index_table, probe_table)
        } else {
            (probe_table, index_table)
        };

        Ok(Self {
            lhs,
            rhs,
            return_index_rows,
            virtual_index_plan,
            virtual_probe_plan,
            virtual_plan,
        })
    }

    /// Find any updates to the tables mentioned by `self` and group them into [`JoinSide`]s.
    ///
    /// The supplied updates are assumed to be the full set of updates from a single transaction.
    ///
    /// If neither side of the join is modified by any of the updates, `None` is returned.
    /// Otherwise, `Some((index_table, probe_table))` is returned
    /// with the updates partitioned into the respective [`JoinSide`].
    fn find_updates<'a>(
        &self,
        updates: impl IntoIterator<Item = &'a DatabaseTableUpdate>,
    ) -> Option<(JoinSide, JoinSide)> {
        let mut lhs_ops = Vec::new();
        let mut rhs_ops = Vec::new();

        for update in updates {
            if update.table_id == self.lhs.table_id {
                lhs_ops.extend(update.ops.iter().cloned());
            } else if update.table_id == self.rhs.table_id {
                rhs_ops.extend(update.ops.iter().cloned());
            }
        }

        if lhs_ops.is_empty() && rhs_ops.is_empty() {
            return None;
        }

        let lhs = JoinSide {
            table_id: self.lhs.table_id,
            table_name: self.lhs.head.table_name.clone(),
            inserts: lhs_ops.iter().filter(|op| op.op_type == 1).cloned().collect(),
            deletes: lhs_ops.iter().filter(|op| op.op_type == 0).cloned().collect(),
        };

        let rhs = JoinSide {
            table_id: self.rhs.table_id,
            table_name: self.rhs.head.table_name.clone(),
            inserts: rhs_ops.iter().filter(|op| op.op_type == 1).cloned().collect(),
            deletes: rhs_ops.iter().filter(|op| op.op_type == 0).cloned().collect(),
        };

        Some((lhs, rhs))
    }

    /// Evaluate join plan for lhs updates.
    fn eval_lhs(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        lhs: DatabaseTableUpdate,
    ) -> Result<impl Iterator<Item = ProductValue>, DBError> {
        let lhs = to_mem_table(self.lhs.head.clone(), self.lhs.table_access, lhs);
        let mut sources = SourceSet::default();
        sources.add_mem_table(lhs);
        eval_updates(db, tx, self.plan_for_delta_lhs(), sources)
    }

    /// Evaluate join plan for rhs updates.
    fn eval_rhs(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        rhs: DatabaseTableUpdate,
    ) -> Result<impl Iterator<Item = ProductValue>, DBError> {
        let rhs = to_mem_table(self.rhs.head.clone(), self.rhs.table_access, rhs);
        let mut sources = SourceSet::default();
        sources.add_mem_table(rhs);
        eval_updates(db, tx, self.plan_for_delta_rhs(), sources)
    }

    /// Evaluate join plan for both lhs and rhs updates.
    fn eval_all(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        lhs: DatabaseTableUpdate,
        rhs: DatabaseTableUpdate,
    ) -> Result<impl Iterator<Item = ProductValue>, DBError> {
        let lhs = to_mem_table(self.lhs.head.clone(), self.lhs.table_access, lhs);
        let rhs = to_mem_table(self.rhs.head.clone(), self.rhs.table_access, rhs);
        let mut sources = SourceSet::default();
        let (index_side, probe_side) = if self.return_index_rows { (lhs, rhs) } else { (rhs, lhs) };
        sources.add_mem_table(index_side);
        sources.add_mem_table(probe_side);
        eval_updates(db, tx, &self.virtual_plan, sources)
    }

    /// Evaluate this [`IncrementalJoin`] over the row updates of a transaction t.
    ///
    /// In the comments that follow,
    /// B(t) refers to the state of table B as of transaction t.
    /// In particular, B(t) includes all of the changes from t.
    /// B(s) refers to the state of table B as of transaction s,
    /// where s is the transaction immediately preceeding t.
    ///
    /// Now we may ask,
    /// given a set of updates to tables A and/or B,
    /// how to efficiently compute the semijoin A(t) x B(t)?
    ///
    /// First consider newly inserted rows of A.
    /// We want to know if they join with any newly inserted rows of B,
    /// or if they join with any previously existing rows of B.
    /// That is:
    ///
    /// A+ x B(t)
    ///
    /// Note that we don't need to consider deleted rows from B.
    /// Because they have no bearing on newly inserted rows of A.
    ///
    /// Now consider rows that were deleted from A.
    /// Similary we want to know if they join with any deleted rows of B,
    /// or if they join with any previously existing rows of B.
    /// That is:
    ///
    /// A- x B(s) U A- x B- = A- x B(t) \ A- x B+ U A- x B-
    ///
    /// Note that we don't necessarily care about newly inserted rows of B in this case.
    /// Because even if they join with deleted rows of A,
    /// they were never included in the results to begin with.
    /// However, during this evaluation, we no longer have direct access to B(s).
    /// Hence we must derive it by subtracting A- x B+ from A- x B(t).
    ///
    /// Finally we must consider previously existing rows of A.
    /// That is:
    ///
    /// A(s) x B+ = A(t) x B+ \ A+ x B+
    /// A(s) x B- = A(t) x B- \ A+ x B-
    ///
    /// In total we must consider 8 distinct joins.
    /// They are:
    ///
    /// (1) A+ x B(t)
    /// (2) A- x B(t)
    /// (3) A- x B+
    /// (4) A- x B-
    /// (5) A(t) x B+
    /// (6) A(t) x B-
    /// (7) A+ x B+
    /// (8) A+ x B-
    pub fn eval<'a>(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        updates: impl IntoIterator<Item = &'a DatabaseTableUpdate>,
    ) -> Result<Option<impl Iterator<Item = TableOp>>, DBError> {
        let Some((lhs, rhs)) = self.find_updates(updates) else {
            return Ok(None);
        };
        // (1) A+ x B(t)
        let join_1 = if lhs.has_inserts() {
            self.eval_lhs(db, tx, lhs.inserts())?.collect()
        } else {
            Vec::with_capacity(0)
        };
        // (2) A- x B(t)
        let mut join_2 = if lhs.has_deletes() {
            self.eval_lhs(db, tx, lhs.deletes())?.collect()
        } else {
            HashSet::with_capacity(0)
        };
        // (3) A- x B+
        let join_3 = if lhs.has_deletes() && rhs.has_inserts() {
            self.eval_all(db, tx, lhs.deletes(), rhs.inserts())?.collect()
        } else {
            Vec::with_capacity(0)
        };
        // (4) A- x B-
        let join_4 = if lhs.has_deletes() && rhs.has_deletes() {
            self.eval_all(db, tx, lhs.deletes(), rhs.deletes())?.collect()
        } else {
            Vec::with_capacity(0)
        };
        // (5) A(t) x B+
        let mut join_5 = if rhs.has_inserts() {
            self.eval_rhs(db, tx, rhs.inserts())?.collect()
        } else {
            HashSet::with_capacity(0)
        };
        // (6) A(t) x B-
        let mut join_6 = if rhs.has_deletes() {
            self.eval_rhs(db, tx, rhs.deletes())?.collect()
        } else {
            HashSet::with_capacity(0)
        };
        // (7) A+ x B+
        let join_7 = if lhs.has_inserts() && rhs.has_inserts() {
            self.eval_all(db, tx, lhs.inserts(), rhs.inserts())?.collect()
        } else {
            Vec::with_capacity(0)
        };
        // (8) A+ x B-
        let join_8 = if lhs.has_inserts() && rhs.has_deletes() {
            self.eval_all(db, tx, lhs.inserts(), rhs.deletes())?.collect()
        } else {
            Vec::with_capacity(0)
        };

        // A- x B(s) = A- x B(t) \ A- x B+
        for row in join_3 {
            join_2.remove(&row);
        }
        // A(s) x B+ = A(t) x B+ \ A+ x B+
        for row in join_7 {
            join_5.remove(&row);
        }
        // A(s) x B- = A(t) x B- \ A+ x B-
        for row in join_8 {
            join_6.remove(&row);
        }

        join_5.retain(|row| !join_6.remove(row));

        Ok(Some(
            join_2
                .into_iter()
                .chain(join_4)
                .chain(join_6)
                .map(TableOp::delete)
                .chain(join_1.into_iter().chain(join_5).map(TableOp::insert)),
        ))
    }
}

/// Construct a [`MemTable`] containing the updates from `delta`,
/// which must be derived from a table with `head` and `table_access`.
fn to_mem_table(head: Arc<Header>, table_access: StAccess, delta: DatabaseTableUpdate) -> MemTable {
    MemTable::new(
        head,
        table_access,
        delta.ops.into_iter().map(|op| op.row).collect::<Vec<_>>(),
    )
}

/// Replace an [IndexJoin]'s scan or fetch operation with a delta table.
/// A delta table consists purely of updates or changes to the base table.
fn with_delta_table(
    mut join: IndexJoin,
    index_side: Option<DatabaseTableUpdate>,
    probe_side: Option<DatabaseTableUpdate>,
) -> (IndexJoin, SourceSet) {
    let mut sources = SourceSet::default();

    if let Some(index_side) = index_side {
        let head = join.index_side.head().clone();
        let table_access = join.index_side.table_access();
        let mem_table = to_mem_table(head, table_access, index_side);
        let source_expr = sources.add_mem_table(mem_table);
        join.index_side = source_expr;
    }

    if let Some(probe_side) = probe_side {
        let head = join.probe_side.source.head().clone();
        let table_access = join.probe_side.source.table_access();
        let mem_table = to_mem_table(head, table_access, probe_side);
        let source_expr = sources.add_mem_table(mem_table);
        join.probe_side.source = source_expr;
    }

    (join, sources)
}

/// A set of independent single or multi-query execution units.
#[derive(Debug, PartialEq, Eq)]
pub struct ExecutionSet {
    exec_units: Vec<Arc<ExecutionUnit>>,
}

impl ExecutionSet {
    #[tracing::instrument(skip_all)]
    pub fn eval(&self, db: &RelationalDB, tx: &Tx) -> Result<DatabaseUpdate, DBError> {
        // evaluate each of the execution units in this ExecutionSet in parallel
        let tables = self
            .exec_units
            // if you need eval to run single-threaded for debugging, change this to .iter()
            .par_iter()
            .filter_map(|unit| unit.eval(db, tx).transpose())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DatabaseUpdate { tables })
    }

    #[tracing::instrument(skip_all)]
    pub fn eval_incr(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        database_update: &DatabaseUpdate,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut tables = Vec::new();
        for unit in &self.exec_units {
            if let Some(table) = unit.eval_incr(db, tx, database_update.tables.iter())? {
                tables.push(table);
            }
        }
        Ok(DatabaseUpdate { tables })
    }
}

impl FromIterator<SupportedQuery> for ExecutionSet {
    fn from_iter<T: IntoIterator<Item = SupportedQuery>>(iter: T) -> Self {
        ExecutionSet {
            exec_units: iter.into_iter().map(|plan| Arc::new(plan.into())).collect(),
        }
    }
}

impl IntoIterator for ExecutionSet {
    type Item = Arc<ExecutionUnit>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.exec_units.into_iter()
    }
}

impl FromIterator<Arc<ExecutionUnit>> for ExecutionSet {
    fn from_iter<T: IntoIterator<Item = Arc<ExecutionUnit>>>(iter: T) -> Self {
        ExecutionSet {
            exec_units: iter.into_iter().collect(),
        }
    }
}

impl From<Vec<Arc<ExecutionUnit>>> for ExecutionSet {
    fn from(value: Vec<Arc<ExecutionUnit>>) -> Self {
        ExecutionSet::from_iter(value)
    }
}

impl From<Vec<SupportedQuery>> for ExecutionSet {
    fn from(value: Vec<SupportedQuery>) -> Self {
        ExecutionSet::from_iter(value)
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
            kind: query::Supported::Select,
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
    use spacetimedb_sats::relation::{DbTable, FieldName};
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::expr::{CrudExpr, IndexJoin, Query, SourceExpr};

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
        let delta = DatabaseTableUpdate {
            table_id: lhs_id,
            table_name: String::from("lhs"),
            ops: vec![TableOp::insert(product![0u64, 0u64])],
        };

        // Optimize the query plan for the incremental update.
        let (expr, _sources) = with_delta_table(join, Some(delta), None);
        let expr: QueryExpr = expr.into();
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
                    source: SourceExpr::MemTable { .. },
                    query: ref lhs,
                },
            probe_field:
                FieldName::Name {
                    table: ref probe_table,
                    field: ref probe_field,
                },
            index_side: SourceExpr::DbTable(DbTable {
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
        let delta = DatabaseTableUpdate {
            table_id: rhs_id,
            table_name: String::from("rhs"),
            ops: vec![TableOp::insert(product![0u64, 0u64, 0u64])],
        };

        // Optimize the query plan for the incremental update.
        let (expr, _sources) = with_delta_table(join, None, Some(delta));
        let expr = QueryExpr::from(expr);
        let mut expr = expr.optimize(&|_, _| i64::MAX);

        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);
        assert!(expr.source.is_db_table());

        let join = expr.query.pop().unwrap();
        let Query::IndexJoin(join) = join else {
            panic!("expected an index join, but got {:#?}", join);
        };

        let IndexJoin {
            probe_side:
                QueryExpr {
                    source: SourceExpr::MemTable { .. },
                    query: ref rhs,
                },
            probe_field:
                FieldName::Name {
                    table: ref probe_table,
                    field: ref probe_field,
                },
            index_side: SourceExpr::DbTable(DbTable {
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

    #[test]
    fn compile_incremental_join_unindexed_semi_join() {
        let (db, _tmp) = make_test_db().expect("Failed to make_test_db");

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let _lhs_id = db
            .create_table_for_test("lhs", schema, indexes)
            .expect("Failed to create_table_for_test lhs");

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[(0.into(), "b"), (1.into(), "c")];
        let _rhs_id = db
            .create_table_for_test("rhs", schema, indexes)
            .expect("Failed to create_table_for_test rhs");

        let tx = db.begin_tx();

        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile_sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(expr.source.table_name(), "lhs");
        assert_eq!(expr.query.len(), 1);

        let src_join = &expr.query[0];
        assert!(
            matches!(src_join, Query::IndexJoin(_)),
            "expected an index join, but got {:#?}",
            src_join
        );

        let incr = IncrementalJoin::new(&expr).expect("Failed to construct IncrementalJoin");

        let virtual_plan = &incr.virtual_plan;

        assert!(virtual_plan.source.is_mem_table());
        assert_eq!(virtual_plan.source.head(), expr.source.head());
        assert_eq!(virtual_plan.query.len(), 1);
        let incr_join = &virtual_plan.query[0];
        let Query::JoinInner(ref incr_join) = incr_join else {
            panic!("expected an inner semijoin, but got {:#?}", incr_join);
        };
        assert!(incr_join.rhs.source.is_mem_table());
        assert_ne!(incr_join.rhs.source.head(), expr.source.head());
        assert!(incr_join.semi);
    }
}
