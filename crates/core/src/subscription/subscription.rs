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
use crate::subscription::query::{to_mem_table_with_op_type, OP_TYPE_FIELD_NAME};
use crate::vm::{build_query, TxMode};
use anyhow::Context;
use itertools::Either;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::ProductValue;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::relation::{DbTable, Header};
use spacetimedb_vm::expr::{self, IndexJoin, Query, QueryCode, QueryExpr, SourceSet};
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

/// Evaluates `query` and returns all the updates to secondary tables only.
///
/// A secondary table is one whose rows are not directly returned by the query.
/// An example is the right table of a left semijoin.
pub fn eval_secondary_updates(
    db: &RelationalDB,
    tx: &Tx,
    query: &QueryCode,
    mut sources: SourceSet,
) -> Result<impl Iterator<Item = ProductValue>, DBError> {
    let ctx = ExecutionContext::incremental_update(db.address());
    let tx: TxMode = tx.into();
    // TODO(perf, 833): avoid clone.
    let query = build_query(&ctx, db, &tx, query.clone(), &mut sources)?;
    // TODO(perf): avoid collecting into a vec.
    Ok(query.collect_vec(|row_ref| row_ref.into_product_value())?.into_iter())
}

/// Evaluates `query` and returns all the updates to its primary table.
///
/// The primary table is the one whose rows are returned by the query.
/// An example is the left table of a left semijoin.
pub fn eval_primary_updates(
    db: &RelationalDB,
    tx: &Tx,
    query: &QueryCode,
    mut sources: SourceSet,
) -> Result<impl Iterator<Item = (u8, ProductValue)>, DBError> {
    let ctx = ExecutionContext::incremental_update(db.address());
    let tx: TxMode = tx.into();
    // TODO(perf, 833): avoid clone.
    let query = build_query(&ctx, db, &tx, query.clone(), &mut sources)?;
    let pos_op_type = query.head().find_pos_by_name(OP_TYPE_FIELD_NAME).unwrap_or_else(|| {
        panic!(
            "Failed to locate `{OP_TYPE_FIELD_NAME}` in `{}`, fields: {:?}",
            query.head().table_name,
            query.head().fields.iter().map(|x| &x.field).collect::<Vec<_>>()
        )
    });
    let pos_op_type = pos_op_type.idx();
    // TODO(perf): avoid collecting into a vec.
    Ok(query
        .collect_vec(|row_ref| {
            let mut row = row_ref.into_product_value();
            let op_type = row
                .elements
                .remove(pos_op_type)
                .into_u8()
                .expect("Failed to extract `{OP_TYPE_FIELD_NAME}` during `eval_primary_updates`");
            (op_type, row)
        })?
        .into_iter())
}

/// Evaluates `query` and returns all the updates
/// either for it primary table, when `is_primary`,
/// or to the secondary tables otherwise.
///
/// The primary table is the one whose rows are returned by the query.
/// An example is the left table of a left semijoin.
fn eval_updates<'a>(
    db: &RelationalDB,
    tx: &Tx,
    query: &QueryCode,
    is_primary: bool,
    sources: SourceSet,
) -> Result<impl 'a + Iterator<Item = ProductValue>, DBError> {
    Ok(if is_primary {
        Either::Left(eval_primary_updates(db, tx, query, sources)?.map(|(_, row)| row))
    } else {
        Either::Right(eval_secondary_updates(db, tx, query, sources)?)
    })
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
/// This gives us 5 different queries to evaluate:
///
/// 1. A+ join B
/// 2. A join B+
/// 3. A- join B
/// 4. A join B-
/// 5. A- join B-
///
/// Among these 5 queries, there are 3 unique plans, as A+ and A- have the same schema, as do B+ and B-.
///
/// For a more in-depth discussion, see the [module-level documentation](./index.html).
#[derive(Debug)]
pub struct IncrementalJoin {
    index_table: DbTable,
    probe_table: DbTable,
    return_index_rows: bool,

    /// A(+/-) join B
    index_modifications_probe_committed: QueryCode,

    /// A join B(+/-)
    index_committed_probe_modifications: QueryCode,

    /// A- join B-
    index_deletes_probe_deletes: QueryCode,
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

    fn optimize_query(join: IndexJoin) -> QueryCode {
        let expr = QueryExpr::from(join);
        // Because (at least) one of the two tables will be a `MemTable`,
        // and therefore not have indexes,
        // the `row_count` function we pass to `optimize` is useless;
        // either the `DbTable` must be used as the index side,
        // or for the `A- join B-` case, the join must be rewritten to not use indexes.
        let expr = expr.optimize(&|_, _| 0);
        spacetimedb_vm::eval::compile_query(expr)
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

        let (index_modifications_probe_committed, _sources) =
            with_delta_table(join.clone(), Some(Self::dummy_table_update(&index_table)), None);
        debug_assert_eq!(_sources.len(), 1);
        let index_modifications_probe_committed = Self::optimize_query(index_modifications_probe_committed);

        let (index_committed_probe_modifications, _sources) =
            with_delta_table(join.clone(), None, Some(Self::dummy_table_update(&probe_table)));
        debug_assert_eq!(_sources.len(), 1);
        let index_committed_probe_modifications = Self::optimize_query(index_committed_probe_modifications);

        let (index_deletes_probe_deletes, _sources) = with_delta_table(
            join.clone(),
            Some(Self::dummy_table_update(&index_table)),
            Some(Self::dummy_table_update(&probe_table)),
        );
        debug_assert_eq!(_sources.len(), 2);
        let index_deletes_probe_deletes = Self::optimize_query(index_deletes_probe_deletes);

        Ok(Self {
            index_table,
            probe_table,
            return_index_rows: join.return_index_rows,

            index_modifications_probe_committed,
            index_committed_probe_modifications,
            index_deletes_probe_deletes,
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
        let mut index_side_updates = Vec::new();
        let mut probe_side_updates = Vec::new();

        for update in updates {
            if update.table_id == self.index_table.table_id {
                index_side_updates.extend(update.ops.iter().cloned());
            } else if update.table_id == self.probe_table.table_id {
                probe_side_updates.extend(update.ops.iter().cloned());
            }
        }

        if index_side_updates.is_empty() && probe_side_updates.is_empty() {
            return None;
        }

        let index_side = JoinSide {
            updates: DatabaseTableUpdate {
                table_id: self.index_table.table_id,
                table_name: self.index_table.head.table_name.clone(),
                ops: index_side_updates,
            },
        };

        let probe_side = JoinSide {
            updates: DatabaseTableUpdate {
                table_id: self.probe_table.table_id,
                table_name: self.probe_table.head.table_name.clone(),
                ops: probe_side_updates,
            },
        };

        Some((index_side, probe_side))
    }

    /// Evaluate this [`IncrementalJoin`].

    pub fn eval<'a>(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        updates: impl IntoIterator<Item = &'a DatabaseTableUpdate>,
    ) -> Result<Option<impl Iterator<Item = TableOp>>, DBError> {
        let Some((index_side, probe_side)) = self.find_updates(updates) else {
            return Ok(None);
        };
        let mut inserts = {
            // {A+ join B}
            let index_inserts = index_side.inserts();
            let index_inserts = to_mem_table_with_op_type(
                self.index_table.head.clone(),
                self.index_table.table_access,
                &index_inserts,
            );
            let mut index_inserts_sources = SourceSet::default();
            index_inserts_sources.add_mem_table(index_inserts);
            let index_insert_results = eval_updates(
                db,
                tx,
                &self.index_modifications_probe_committed,
                self.return_index_rows,
                index_inserts_sources,
            )?;

            // {A join B+}
            let probe_inserts = probe_side.inserts();
            let probe_inserts = to_mem_table_with_op_type(
                self.probe_table.head.clone(),
                self.probe_table.table_access,
                &probe_inserts,
            );
            let mut probe_inserts_sources = SourceSet::default();
            probe_inserts_sources.add_mem_table(probe_inserts);
            let probe_insert_results = eval_updates(
                db,
                tx,
                &self.index_committed_probe_modifications,
                !self.return_index_rows,
                probe_inserts_sources,
            )?;

            // {A+ join B} U {A join B+}
            itertools::chain![index_insert_results, probe_insert_results].collect::<HashSet<_>>()
        };
        let mut deletes = {
            // {A- join B}
            let index_deletes = index_side.deletes();
            let index_deletes = to_mem_table_with_op_type(
                self.index_table.head.clone(),
                self.index_table.table_access,
                &index_deletes,
            );
            let mut index_deletes_sources = SourceSet::default();
            index_deletes_sources.add_mem_table(index_deletes);
            let index_deletes_results = eval_updates(
                db,
                tx,
                &self.index_modifications_probe_committed,
                self.return_index_rows,
                index_deletes_sources,
            )?;

            // {A join B-}
            let probe_deletes = probe_side.deletes();
            let probe_deletes = to_mem_table_with_op_type(
                self.probe_table.head.clone(),
                self.probe_table.table_access,
                &probe_deletes,
            );
            let mut probe_deletes_sources = SourceSet::default();
            probe_deletes_sources.add_mem_table(probe_deletes);
            let probe_delete_results = eval_updates(
                db,
                tx,
                &self.index_committed_probe_modifications,
                !self.return_index_rows,
                probe_deletes_sources,
            )?;

            // {A- join B-}
            let index_deletes = index_side.deletes();
            let index_deletes = to_mem_table_with_op_type(
                self.index_table.head.clone(),
                self.index_table.table_access,
                &index_deletes,
            );
            let probe_deletes = probe_side.deletes();
            let probe_deletes = to_mem_table_with_op_type(
                self.probe_table.head.clone(),
                self.probe_table.table_access,
                &probe_deletes,
            );
            let mut both_deletes_sources = SourceSet::default();
            both_deletes_sources.add_mem_table(index_deletes);
            both_deletes_sources.add_mem_table(probe_deletes);
            let both_delete_results =
                eval_updates(db, tx, &self.index_deletes_probe_deletes, true, both_deletes_sources)?;

            // {A- join B} U {A join B-} U {A- join B-}
            itertools::chain![index_deletes_results, probe_delete_results, both_delete_results].collect::<HashSet<_>>()
        };

        deletes.retain(|row| !inserts.remove(row));

        // Deletes need to come first, as UPDATE = [DELETE, INSERT]
        Ok(Some(
            deletes
                .into_iter()
                .map(TableOp::delete)
                .chain(inserts.into_iter().map(TableOp::insert)),
        ))
    }
}

/// Replace an [IndexJoin]'s scan or fetch operation with a delta table.
/// A delta table consists purely of updates or changes to the base table.
fn with_delta_table(
    mut join: IndexJoin,
    index_side: Option<DatabaseTableUpdate>,
    probe_side: Option<DatabaseTableUpdate>,
) -> (IndexJoin, SourceSet) {
    fn to_mem_table(head: Arc<Header>, table_access: StAccess, delta: DatabaseTableUpdate) -> MemTable {
        MemTable::new(
            head,
            table_access,
            delta.ops.into_iter().map(|op| op.row).collect::<Vec<_>>(),
        )
    }

    let mut sources = SourceSet::default();

    if let Some(index_side) = index_side {
        let head = join.index_side.head().clone();
        let table_access = join.index_side.table_access();
        let mem_table = if join.return_index_rows {
            // We are replacing the indexed table,
            // and the rows of the indexed table are being returned.
            // Therefore we must add a column with the op type.
            to_mem_table_with_op_type(head, table_access, &index_side)
        } else {
            // We are replacing the indexed table,
            // but the rows of the indexed table are not being returned.
            // Therefore we do not need to add a column with the op type.
            to_mem_table(head, table_access, index_side)
        };
        let source_expr = sources.add_mem_table(mem_table);
        join.index_side = source_expr;
    }

    if let Some(probe_side) = probe_side {
        let head = join.probe_side.source.head().clone();
        let table_access = join.probe_side.source.table_access();
        let mem_table = if join.return_index_rows {
            // We are replacing the probe table,
            // but the rows of the indexed table are being returned.
            // Therefore we do not need to add a column with the op type.
            to_mem_table(head, table_access, probe_side)
        } else {
            // We are replacing the probe table,
            // and the rows of the probe table are being returned.
            // Therefore we must add a column with the op type.
            to_mem_table_with_op_type(head, table_access, &probe_side)
        };
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
        let tables = self
            .exec_units
            .iter()
            .filter_map(|unit| unit.eval_incr(db, tx, database_update.tables.iter()).transpose())
            .collect::<Result<_, _>>()?;
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
}
