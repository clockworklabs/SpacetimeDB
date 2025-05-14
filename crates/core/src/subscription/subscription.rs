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
//! of join query _can_ be evaluated incrementally without materialized views.

use super::execution_unit::{ExecutionUnit, QueryHash};
use super::module_subscription_manager::Plan;
use super::query;
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdateRelValue, UpdatesRelValue};
use crate::messages::websocket as ws;
use crate::sql::ast::SchemaViewer;
use crate::vm::{build_query, TxMode};
use anyhow::Context;
use itertools::Either;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{Compression, WebsocketFormat};
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::error::AuthError;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::DbTable;
use spacetimedb_lib::{Identity, ProductValue};
use spacetimedb_primitives::TableId;
use spacetimedb_subscription::SubscriptionPlan;
use spacetimedb_vm::expr::{self, AuthAccess, IndexJoin, Query, QueryExpr, SourceExpr, SourceProvider, SourceSet};
use spacetimedb_vm::rel_ops::RelOps;
use spacetimedb_vm::relation::{MemTable, RelValue};
use std::hash::Hash;
use std::iter;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

/// A [`QueryExpr`] tagged with [`query::Supported`].
///
/// Constructed via `TryFrom`, which rejects unsupported queries.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SupportedQuery {
    pub kind: query::Supported,
    pub expr: QueryExpr,
    pub sql: String,
}

impl SupportedQuery {
    pub fn new(expr: QueryExpr, sql: String) -> Result<Self, DBError> {
        let kind = query::classify(&expr).ok_or_else(|| SubscriptionError::Unsupported(sql.clone()))?;
        Ok(Self { kind, expr, sql })
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
impl TryFrom<(QueryExpr, String)> for SupportedQuery {
    type Error = DBError;

    fn try_from((expr, sql): (QueryExpr, String)) -> Result<Self, Self::Error> {
        let kind = query::classify(&expr).context("Unsupported query expression")?;
        Ok(Self { kind, expr, sql })
    }
}

impl AsRef<QueryExpr> for SupportedQuery {
    fn as_ref(&self) -> &QueryExpr {
        &self.expr
    }
}

/// Evaluates `query` and returns all the updates.
fn eval_updates<'a>(
    db: &'a RelationalDB,
    tx: &'a TxMode<'a>,
    query: &'a QueryExpr,
    mut sources: impl SourceProvider<'a>,
) -> impl 'a + Iterator<Item = RelValue<'a>> {
    let mut query = build_query(db, tx, query, &mut sources);
    iter::from_fn(move || query.next())
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

impl IncrementalJoin {
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

        let (virtual_index_plan, _sources) = with_delta_table(join.clone(), Some(Vec::new()), None);
        debug_assert_eq!(_sources.len(), 1);
        let virtual_index_plan = Self::optimize_query(virtual_index_plan);

        let (virtual_probe_plan, _sources) = with_delta_table(join.clone(), None, Some(Vec::new()));
        debug_assert_eq!(_sources.len(), 1);
        let virtual_probe_plan = Self::optimize_query(virtual_probe_plan);

        let (virtual_plan, _sources) = with_delta_table(join.clone(), Some(Vec::new()), Some(Vec::new()));
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

    /// Evaluate join plan for lhs updates.
    fn eval_lhs<'a>(
        &'a self,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        lhs: impl 'a + Iterator<Item = &'a ProductValue>,
    ) -> impl Iterator<Item = RelValue<'a>> {
        eval_updates(db, tx, self.plan_for_delta_lhs(), Some(lhs.map(RelValue::ProjRef)))
    }

    /// Evaluate join plan for rhs updates.
    fn eval_rhs<'a>(
        &'a self,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        rhs: impl 'a + Iterator<Item = &'a ProductValue>,
    ) -> impl Iterator<Item = RelValue<'a>> {
        eval_updates(db, tx, self.plan_for_delta_rhs(), Some(rhs.map(RelValue::ProjRef)))
    }

    /// Evaluate join plan for both lhs and rhs updates.
    fn eval_all<'a>(
        &'a self,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        lhs: impl 'a + Iterator<Item = &'a ProductValue>,
        rhs: impl 'a + Iterator<Item = &'a ProductValue>,
    ) -> impl Iterator<Item = RelValue<'a>> {
        let is = Either::Left(lhs.map(RelValue::ProjRef));
        let ps = Either::Right(rhs.map(RelValue::ProjRef));
        let sources: SourceSet<_, 2> = if self.return_index_rows { [is, ps] } else { [ps, is] }.into();
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
        &'a self,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        updates: impl 'a + Clone + Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> UpdatesRelValue<'a> {
        // Find any updates to the tables mentioned by `self` and group them into [`JoinSide`]s.
        //
        // The supplied updates are assumed to be the full set of updates from a single transaction.
        //
        // If neither side of the join is modified by any of the updates, `None` is returned.
        // Otherwise, `Some((index_table, probe_table))` is returned
        // with the updates partitioned into the respective [`JoinSide`].
        // =====================================================================

        // Partitions `updates` into `deletes` and `inserts` for `lhs` and `rhs`.
        let mut lhs_deletes = updates
            .clone()
            .filter(|u| u.table_id == self.lhs.table_id)
            .flat_map(|u| u.deletes.iter())
            .peekable();
        let mut lhs_inserts = updates
            .clone()
            .filter(|u| u.table_id == self.lhs.table_id)
            .flat_map(|u| u.inserts.iter())
            .peekable();
        let mut rhs_deletes = updates
            .clone()
            .filter(|u| u.table_id == self.rhs.table_id)
            .flat_map(|u| u.deletes.iter())
            .peekable();
        let mut rhs_inserts = updates
            .filter(|u| u.table_id == self.rhs.table_id)
            .flat_map(|u| u.inserts.iter())
            .peekable();

        // No updates at all? Return `None`.
        let has_lhs_deletes = lhs_deletes.peek().is_some();
        let has_lhs_inserts = lhs_inserts.peek().is_some();
        let has_rhs_deletes = rhs_deletes.peek().is_some();
        let has_rhs_inserts = rhs_inserts.peek().is_some();
        if !has_lhs_deletes && !has_lhs_inserts && !has_rhs_deletes && !has_rhs_inserts {
            return <_>::default();
        }

        // Compute the incremental join
        // =====================================================================

        fn collect_set<T: Hash + Eq, I: Iterator<Item = T>>(
            produce_if: bool,
            producer: impl FnOnce() -> I,
        ) -> HashSet<T> {
            if produce_if {
                producer().collect()
            } else {
                HashSet::default()
            }
        }

        fn make_iter<T, I: Iterator<Item = T>>(
            produce_if: bool,
            producer: impl FnOnce() -> I,
        ) -> impl Iterator<Item = T> {
            if produce_if {
                Either::Left(producer())
            } else {
                Either::Right(iter::empty())
            }
        }

        // (1) A+ x B(t)
        let j1_lhs_ins = lhs_inserts.clone();
        let join_1 = make_iter(has_lhs_inserts, || self.eval_lhs(db, tx, j1_lhs_ins));
        // (2) A- x B(t)
        let j2_lhs_del = lhs_deletes.clone();
        let mut join_2 = collect_set(has_lhs_deletes, || self.eval_lhs(db, tx, j2_lhs_del));
        // (3) A- x B+
        let j3_lhs_del = lhs_deletes.clone();
        let j3_rhs_ins = rhs_inserts.clone();
        let join_3 = make_iter(has_lhs_deletes && has_rhs_inserts, || {
            self.eval_all(db, tx, j3_lhs_del, j3_rhs_ins)
        });
        // (4) A- x B-
        let j4_rhs_del = rhs_deletes.clone();
        let join_4 = make_iter(has_lhs_deletes && has_rhs_deletes, || {
            self.eval_all(db, tx, lhs_deletes, j4_rhs_del)
        });
        // (5) A(t) x B+
        let j5_rhs_ins = rhs_inserts.clone();
        let mut join_5 = collect_set(has_rhs_inserts, || self.eval_rhs(db, tx, j5_rhs_ins));
        // (6) A(t) x B-
        let j6_rhs_del = rhs_deletes.clone();
        let mut join_6 = collect_set(has_rhs_deletes, || self.eval_rhs(db, tx, j6_rhs_del));
        // (7) A+ x B+
        let j7_lhs_ins = lhs_inserts.clone();
        let join_7 = make_iter(has_lhs_inserts && has_rhs_inserts, || {
            self.eval_all(db, tx, j7_lhs_ins, rhs_inserts)
        });
        // (8) A+ x B-
        let join_8 = make_iter(has_lhs_inserts && has_rhs_deletes, || {
            self.eval_all(db, tx, lhs_inserts, rhs_deletes)
        });

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

        // Collect deletes:
        let mut deletes = Vec::new();
        deletes.extend(join_2);
        for row in join_4 {
            deletes.push(row);
        }
        deletes.extend(join_6);

        // Collect inserts:
        let mut inserts = Vec::new();
        for row in join_1 {
            inserts.push(row);
        }
        inserts.extend(join_5);

        UpdatesRelValue { deletes, inserts }
    }
}

/// Replace an [IndexJoin]'s scan or fetch operation with a delta table.
/// A delta table consists purely of updates or changes to the base table.
fn with_delta_table(
    mut join: IndexJoin,
    index_side: Option<Vec<ProductValue>>,
    probe_side: Option<Vec<ProductValue>>,
) -> (IndexJoin, SourceSet<Vec<ProductValue>, 2>) {
    let mut sources = SourceSet::empty();

    let mut add_mem_table =
        |side: SourceExpr, data| sources.add_mem_table(MemTable::new(side.head().clone(), side.table_access(), data));

    if let Some(index_side) = index_side {
        join.index_side = add_mem_table(join.index_side, index_side);
    }

    if let Some(probe_side) = probe_side {
        join.probe_side.source = add_mem_table(join.probe_side.source, probe_side);
    }

    (join, sources)
}

/// A set of independent single or multi-query execution units.
#[derive(Debug, PartialEq, Eq)]
pub struct ExecutionSet {
    exec_units: Vec<Arc<ExecutionUnit>>,
}

impl ExecutionSet {
    pub fn eval<F: WebsocketFormat>(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        slow_query_threshold: Option<Duration>,
        compression: Compression,
    ) -> ws::DatabaseUpdate<F> {
        // evaluate each of the execution units in this ExecutionSet in parallel
        let tables = self
            .exec_units
            // if you need eval to run single-threaded for debugging, change this to .iter()
            .par_iter()
            .filter_map(|unit| unit.eval(db, tx, &unit.sql, slow_query_threshold, compression))
            .collect();
        ws::DatabaseUpdate { tables }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn eval_incr_for_test<'a>(
        &'a self,

        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        database_update: &'a [&'a DatabaseTableUpdate],
        slow_query_threshold: Option<Duration>,
    ) -> DatabaseUpdateRelValue<'a> {
        let mut tables = Vec::new();
        for unit in &self.exec_units {
            if let Some(table) =
                unit.eval_incr(db, tx, &unit.sql, database_update.iter().copied(), slow_query_threshold)
            {
                tables.push(table);
            }
        }

        DatabaseUpdateRelValue { tables }
    }

    /// The estimated number of rows returned by this execution set.
    pub fn row_estimate(&self, tx: &TxId) -> u64 {
        self.exec_units
            .iter()
            .map(|unit| unit.row_estimate(tx))
            .fold(0, |acc, est| acc.saturating_add(est))
    }

    /// Return an iterator over the execution units
    pub fn iter(&self) -> impl Iterator<Item = &ExecutionUnit> {
        self.exec_units.iter().map(|arc| &**arc)
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

impl AuthAccess for ExecutionSet {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        self.exec_units.iter().try_for_each(|eu| eu.check_auth(owner, caller))
    }
}

/// Queries all the [`StTableType::User`] tables *right now*
/// and turns them into [`QueryExpr`],
/// the moral equivalent of `SELECT * FROM table`.
pub(crate) fn get_all(relational_db: &RelationalDB, tx: &Tx, auth: &AuthCtx) -> Result<Vec<Plan>, DBError> {
    Ok(relational_db
        .get_all_tables(tx)?
        .iter()
        .map(Deref::deref)
        .filter(|t| {
            t.table_type == StTableType::User && (auth.owner == auth.caller || t.table_access == StAccess::Public)
        })
        .map(|schema| {
            let sql = format!("SELECT * FROM {}", schema.table_name);
            SubscriptionPlan::compile(&sql, &SchemaViewer::new(tx, auth), auth)
                .map(|(plans, has_param)| Plan::new(plans, QueryHash::from_string(&sql, auth.caller, has_param), sql))
        })
        .collect::<Result<_, _>>()?)
}

/// Queries all the [`StTableType::User`] tables *right now*
/// and turns them into [`QueryExpr`],
/// the moral equivalent of `SELECT * FROM table`.
#[cfg(test)]
pub(crate) fn legacy_get_all(
    relational_db: &RelationalDB,
    tx: &Tx,
    auth: &AuthCtx,
) -> Result<Vec<SupportedQuery>, DBError> {
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
            sql: format!("SELECT * FROM {}", src.table_name),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::{begin_tx, TestDB};
    use crate::sql::compiler::compile_sql;
    use spacetimedb_lib::relation::DbTable;
    use spacetimedb_lib::{error::ResultTest, identity::AuthCtx};
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::expr::{CrudExpr, IndexJoin, Query, SourceExpr};

    #[test]
    // Compile an index join after replacing the index side with a virtual table.
    // The original index and probe sides should be swapped after introducing the delta table.
    fn compile_incremental_index_join_index_side() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let _ = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[0.into(), 1.into()];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

        let tx = begin_tx(&db);
        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &AuthCtx::for_testing(), &tx, sql)?.remove(0);

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
        let delta = vec![product![0u64, 0u64]];

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
                    source: SourceExpr::InMemory { .. },
                    query: ref lhs,
                },
            probe_col,
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
        assert_eq!(probe_col, 1.into());
        Ok(())
    }

    #[test]
    // Compile an index join after replacing the probe side with a virtual table.
    // The original index and probe sides should remain after introducing the virtual table.
    fn compile_incremental_index_join_probe_side() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[0.into(), 1.into()];
        let _ = db.create_table_for_test("rhs", schema, indexes)?;

        let tx = begin_tx(&db);
        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &AuthCtx::for_testing(), &tx, sql)?.remove(0);

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
        let delta = vec![product![0u64, 0u64, 0u64]];

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
                    source: SourceExpr::InMemory { .. },
                    query: ref rhs,
                },
            probe_col,
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
        assert_eq!(probe_col, 0.into());
        Ok(())
    }

    #[test]
    fn compile_incremental_join_unindexed_semi_join() {
        let db = TestDB::durable().expect("failed to make test db");

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let _lhs_id = db
            .create_table_for_test("lhs", schema, indexes)
            .expect("Failed to create_table_for_test lhs");

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[0.into(), 1.into()];
        let _rhs_id = db
            .create_table_for_test("rhs", schema, indexes)
            .expect("Failed to create_table_for_test rhs");

        let tx = begin_tx(&db);

        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &AuthCtx::for_testing(), &tx, sql)
            .expect("Failed to compile_sql")
            .remove(0);

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
        assert_eq!(virtual_plan.head(), expr.head());
        assert_eq!(virtual_plan.query.len(), 1);
        let incr_join = &virtual_plan.query[0];
        let Query::JoinInner(ref incr_join) = incr_join else {
            panic!("expected an inner semijoin, but got {:#?}", incr_join);
        };
        assert!(incr_join.rhs.source.is_mem_table());
        assert_ne!(incr_join.rhs.source.head(), expr.source.head());
        assert_ne!(incr_join.rhs.head(), expr.head());
        assert_eq!(incr_join.inner, None);
    }
}
