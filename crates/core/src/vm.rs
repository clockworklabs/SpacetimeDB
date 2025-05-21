//! The [DbProgram] that execute arbitrary queries & code against the database.

use crate::db::datastore::locking_tx_datastore::state_view::IterByColRangeMutTx;
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::datastore::locking_tx_datastore::IterByColRangeTx;
use crate::db::datastore::system_tables::{st_var_schema, StVarName, StVarRow, StVarTable};
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation;
use crate::execution_context::ExecutionContext;
use core::ops::{Bound, RangeBounds};
use itertools::Itertools;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::{ColExpr, DbTable};
use spacetimedb_primitives::*;
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_table::static_assert_size;
use spacetimedb_table::table::RowRef;
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::eval::{box_iter, build_project, build_select, join_inner, IterRows};
use spacetimedb_vm::expr::*;
use spacetimedb_vm::iterators::RelIter;
use spacetimedb_vm::program::{ProgramVm, Sources};
use spacetimedb_vm::rel_ops::{EmptyRelOps, RelOps};
use spacetimedb_vm::relation::{MemTable, RelValue};
use std::str::FromStr;
use std::sync::Arc;

pub enum TxMode<'a> {
    MutTx(&'a mut MutTx),
    Tx(&'a Tx),
}

impl TxMode<'_> {
    /// Unwraps `self`, ensuring we are in a mutable tx.
    fn unwrap_mut(&mut self) -> &mut MutTx {
        match self {
            Self::MutTx(tx) => tx,
            Self::Tx(_) => unreachable!("mutable operation is invalid with read tx"),
        }
    }

    pub(crate) fn ctx(&self) -> &ExecutionContext {
        match self {
            Self::MutTx(tx) => &tx.ctx,
            Self::Tx(tx) => &tx.ctx,
        }
    }
}

impl<'a> From<&'a mut MutTx> for TxMode<'a> {
    fn from(tx: &'a mut MutTx) -> Self {
        TxMode::MutTx(tx)
    }
}

impl<'a> From<&'a Tx> for TxMode<'a> {
    fn from(tx: &'a Tx) -> Self {
        TxMode::Tx(tx)
    }
}

impl<'a> From<&'a mut Tx> for TxMode<'a> {
    fn from(tx: &'a mut Tx) -> Self {
        TxMode::Tx(tx)
    }
}

fn bound_is_satisfiable(lower: &Bound<AlgebraicValue>, upper: &Bound<AlgebraicValue>) -> bool {
    match (lower, upper) {
        (Bound::Excluded(lower), Bound::Excluded(upper)) if lower >= upper => false,
        (Bound::Included(lower), Bound::Excluded(upper)) | (Bound::Excluded(lower), Bound::Included(upper))
            if lower > upper =>
        {
            false
        }
        _ => true,
    }
}

//TODO: This is partially duplicated from the `vm` crate to avoid borrow checker issues
//and pull all that crate in core. Will be revisited after trait refactor
pub fn build_query<'a>(
    db: &'a RelationalDB,
    tx: &'a TxMode<'a>,
    query: &'a QueryExpr,
    sources: &mut impl SourceProvider<'a>,
) -> Box<IterRows<'a>> {
    let db_table = query.source.is_db_table();

    // We're incrementally building a query iterator by applying each operation in the `query.query`.
    // Most such operations will modify their parent, but certain operations (i.e. `IndexJoin`s)
    // are only valid as the first operation in the list,
    // and construct a new base query.
    //
    // Branches which use `result` will do `unwrap_or_else(|| get_table(ctx, db, tx, &query.table, sources))`
    // to get an `IterRows` defaulting to the `query.table`.
    //
    // Branches which do not use the `result` will assert that it is `None`,
    // i.e. that they are the first operator.
    //
    // TODO(bikeshedding): Avoid duplication of the ugly `result.take().map(...).unwrap_or_else(...)?` expr?
    // TODO(bikeshedding): Refactor `QueryExpr` to separate `IndexJoin` from other `Query` variants,
    //   removing the need for this convoluted logic?
    let mut result = None;

    let result_or_base = |sources: &mut _, result: &mut Option<_>| {
        result
            .take()
            .unwrap_or_else(|| get_table(db, tx, &query.source, sources))
    };

    for op in &query.query {
        result = Some(match op {
            Query::IndexScan(IndexScan { table, columns, bounds }) if db_table => {
                if !bound_is_satisfiable(&bounds.0, &bounds.1) {
                    // If the bound is impossible to satisfy
                    // because the lower bound is greater than the upper bound, or both bounds are excluded and equal,
                    // return an empty iterator.
                    // This avoids a panic in `BTreeMap`'s `NodeRef::search_tree_for_bifurcation`,
                    // which is very unhappy about unsatisfiable bounds.
                    Box::new(EmptyRelOps) as Box<IterRows<'a>>
                } else {
                    let bounds = (bounds.start_bound(), bounds.end_bound());
                    iter_by_col_range(db, tx, table, columns.clone(), bounds)
                }
            }
            Query::IndexScan(index_scan) => {
                let result = result_or_base(sources, &mut result);
                let cols = &index_scan.columns;
                let bounds = &index_scan.bounds;

                if !bound_is_satisfiable(&bounds.0, &bounds.1) {
                    // If the bound is impossible to satisfy
                    // because the lower bound is greater than the upper bound, or both bounds are excluded and equal,
                    // return an empty iterator.
                    // Unlike the above case, this is not necessary, as the below `select` will never panic,
                    // but it's still nice to avoid needlessly traversing a bunch of rows.
                    // TODO: We should change the compiler to not emit an `IndexScan` in this case,
                    // so that this branch is unreachable.
                    // The current behavior is a hack
                    // because this patch was written (2024-04-01 pgoldman) a short time before the BitCraft alpha,
                    // and a more invasive change was infeasible.
                    Box::new(EmptyRelOps) as Box<IterRows<'a>>
                } else if let Some(head) = cols.as_singleton() {
                    // For singleton constraints, we compare the column directly against `bounds`.
                    let head = head.idx();
                    let iter = result.select(move |row| bounds.contains(&*row.read_column(head).unwrap()));
                    Box::new(iter) as Box<IterRows<'a>>
                } else {
                    // For multi-col constraints, these are stored as bounds of product values,
                    // so we need to project these into single-col bounds and compare against the column.
                    // Project start/end `Bound<AV>`s to `Bound<Vec<AV>>`s.
                    let start_bound = bounds.0.as_ref().map(|av| &av.as_product().unwrap().elements);
                    let end_bound = bounds.1.as_ref().map(|av| &av.as_product().unwrap().elements);
                    // Construct the query:
                    Box::new(result.select(move |row| {
                        // Go through each column position,
                        // project to a `Bound<AV>` for the position,
                        // and compare against the column in the row.
                        // All columns must match to include the row,
                        // which is essentially the same as a big `AND` of `ColumnOp`s.
                        cols.iter().enumerate().all(|(idx, col)| {
                            let start_bound = start_bound.map(|pv| &pv[idx]);
                            let end_bound = end_bound.map(|pv| &pv[idx]);
                            let read_col = row.read_column(col.idx()).unwrap();
                            (start_bound, end_bound).contains(&*read_col)
                        })
                    }))
                }
            }
            Query::IndexJoin(_) if result.is_some() => panic!("Invalid query: `IndexJoin` must be the first operator"),
            Query::IndexJoin(IndexJoin {
                probe_side,
                probe_col,
                index_side,
                index_select,
                index_col,
                return_index_rows,
            }) => {
                let probe_side = build_query(db, tx, probe_side, sources);
                // The compiler guarantees that the index side is a db table,
                // and therefore this unwrap is always safe.
                let index_table = index_side.table_id().unwrap();

                if *return_index_rows {
                    index_semi_join_left(db, tx, probe_side, *probe_col, index_select, index_table, *index_col)
                } else {
                    index_semi_join_right(db, tx, probe_side, *probe_col, index_select, index_table, *index_col)
                }
            }
            Query::Select(cmp) => build_select(result_or_base(sources, &mut result), cmp),
            Query::Project(proj) => build_project(result_or_base(sources, &mut result), proj),
            Query::JoinInner(join) => join_inner(
                result_or_base(sources, &mut result),
                build_query(db, tx, &join.rhs, sources),
                join,
            ),
        })
    }

    result_or_base(sources, &mut result)
}

/// Resolve `query` to a table iterator,
/// either taken from an in-memory table, in the case of [`SourceExpr::InMemory`],
/// or from a physical table, in the case of [`SourceExpr::DbTable`].
///
/// If `query` refers to an in memory table,
/// `sources` will be used to fetch the table `I`.
/// Examples of `I` could be derived from `MemTable` or `&'a [ProductValue]`
/// whereas `sources` could a [`SourceSet`].
///
/// On the other hand, if the `query` is a `SourceExpr::DbTable`, `sources` is unused.
fn get_table<'a>(
    stdb: &'a RelationalDB,
    tx: &'a TxMode,
    query: &'a SourceExpr,
    sources: &mut impl SourceProvider<'a>,
) -> Box<IterRows<'a>> {
    match query {
        // Extracts an in-memory table with `source_id` from `sources` and builds a query for the table.
        SourceExpr::InMemory { source_id, .. } => build_iter(
            sources
                .take_source(*source_id)
                .unwrap_or_else(|| {
                    panic!("Query plan specifies in-mem table for {source_id:?}, but found a `DbTable` or nothing")
                })
                .into_iter(),
        ),
        SourceExpr::DbTable(db_table) => build_iter_from_db(match tx {
            TxMode::MutTx(tx) => stdb.iter_mut(tx, db_table.table_id).map(box_iter),
            TxMode::Tx(tx) => stdb.iter(tx, db_table.table_id).map(box_iter),
        }),
    }
}

fn iter_by_col_range<'a>(
    db: &'a RelationalDB,
    tx: &'a TxMode,
    table: &'a DbTable,
    columns: ColList,
    range: impl RangeBounds<AlgebraicValue> + 'a,
) -> Box<IterRows<'a>> {
    build_iter_from_db(match tx {
        TxMode::MutTx(tx) => db
            .iter_by_col_range_mut(tx, table.table_id, columns, range)
            .map(box_iter),
        TxMode::Tx(tx) => db.iter_by_col_range(tx, table.table_id, columns, range).map(box_iter),
    })
}

fn build_iter_from_db<'a>(iter: Result<impl 'a + Iterator<Item = RowRef<'a>>, DBError>) -> Box<IterRows<'a>> {
    build_iter(iter.expect(TABLE_ID_EXPECTED_VALID).map(RelValue::Row))
}

fn build_iter<'a>(iter: impl 'a + Iterator<Item = RelValue<'a>>) -> Box<IterRows<'a>> {
    Box::new(RelIter::new(iter)) as Box<IterRows<'_>>
}

const TABLE_ID_EXPECTED_VALID: &str = "all `table_id`s in compiled query should be valid";

/// An index join operator that returns matching rows from the index side.
pub struct IndexSemiJoinLeft<'c, Rhs, IndexIter, F> {
    /// An iterator for the probe side.
    /// The values returned will be used to probe the index.
    probe_side: Rhs,
    /// The column whose value will be used to probe the index.
    probe_col: ColId,
    /// An optional predicate to evaluate over the matching rows of the index.
    index_select: &'c Option<ColumnOp>,
    /// An iterator for the index side.
    /// A new iterator will be instantiated for each row on the probe side.
    index_iter: Option<IndexIter>,
    /// The function that returns an iterator for the index side.
    index_function: F,
}

impl<'a, Rhs, IndexIter, F> IndexSemiJoinLeft<'_, Rhs, IndexIter, F>
where
    F: Fn(AlgebraicValue) -> Result<IndexIter, DBError>,
    IndexIter: Iterator<Item = RowRef<'a>>,
    Rhs: RelOps<'a>,
{
    fn filter(&self, index_row: &RelValue<'_>) -> bool {
        self.index_select.as_ref().map_or(true, |op| op.eval_bool(index_row))
    }
}

impl<'a, Rhs, IndexIter, F> RelOps<'a> for IndexSemiJoinLeft<'_, Rhs, IndexIter, F>
where
    F: Fn(AlgebraicValue) -> Result<IndexIter, DBError>,
    IndexIter: Iterator<Item = RowRef<'a>>,
    Rhs: RelOps<'a>,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        // Return a value from the current index iterator, if not exhausted.
        while let Some(index_row) = self.index_iter.as_mut().and_then(|iter| iter.next()).map(RelValue::Row) {
            if self.filter(&index_row) {
                return Some(index_row);
            }
        }

        // Otherwise probe the index with a row from the probe side.
        let probe_col = self.probe_col.idx();
        while let Some(mut row) = self.probe_side.next() {
            if let Some(value) = row.read_or_take_column(probe_col) {
                let mut index_iter = (self.index_function)(value).expect(TABLE_ID_EXPECTED_VALID);
                while let Some(index_row) = index_iter.next().map(RelValue::Row) {
                    if self.filter(&index_row) {
                        self.index_iter = Some(index_iter);
                        return Some(index_row);
                    }
                }
            }
        }
        None
    }
}

/// Return an iterator index join operator that returns matching rows from the index side.
pub fn index_semi_join_left<'a>(
    db: &'a RelationalDB,
    tx: &'a TxMode<'a>,
    probe_side: Box<IterRows<'a>>,
    probe_col: ColId,
    index_select: &'a Option<ColumnOp>,
    index_table: TableId,
    index_col: ColId,
) -> Box<IterRows<'a>> {
    match tx {
        TxMode::MutTx(tx) => Box::new(IndexSemiJoinLeft {
            probe_side,
            probe_col,
            index_select,
            index_iter: None,
            index_function: move |value| db.iter_by_col_range_mut(tx, index_table, index_col, value),
        }),
        TxMode::Tx(tx) => Box::new(IndexSemiJoinLeft {
            probe_side,
            probe_col,
            index_select,
            index_iter: None,
            index_function: move |value| db.iter_by_col_range(tx, index_table, index_col, value),
        }),
    }
}

static_assert_size!(
    IndexSemiJoinLeft<
        Box<IterRows<'static>>,
        fn(AlgebraicValue) -> Result<IterByColRangeTx<'static, AlgebraicValue>, DBError>,
        IterByColRangeTx<'static, AlgebraicValue>,
    >,
    144
);
static_assert_size!(
    IndexSemiJoinLeft<
        Box<IterRows<'static>>,
        fn(AlgebraicValue) -> Result<IterByColRangeMutTx<'static, AlgebraicValue>, DBError>,
        IterByColRangeMutTx<'static, AlgebraicValue>,
    >,
    240
);

/// An index join operator that returns matching rows from the probe side.
pub struct IndexSemiJoinRight<'c, Rhs: RelOps<'c>, F> {
    /// An iterator for the probe side.
    /// The values returned will be used to probe the index.
    probe_side: Rhs,
    /// The column whose value will be used to probe the index.
    probe_col: ColId,
    /// An optional predicate to evaluate over the matching rows of the index.
    index_select: &'c Option<ColumnOp>,
    /// A function that returns an iterator for the index side.
    index_function: F,
}

impl<'a, Rhs: RelOps<'a>, F, IndexIter> IndexSemiJoinRight<'a, Rhs, F>
where
    F: Fn(AlgebraicValue) -> Result<IndexIter, DBError>,
    IndexIter: Iterator<Item = RowRef<'a>>,
{
    fn filter(&self, index_row: &RelValue<'_>) -> bool {
        self.index_select.as_ref().map_or(true, |op| op.eval_bool(index_row))
    }
}

impl<'a, Rhs: RelOps<'a>, F, IndexIter> RelOps<'a> for IndexSemiJoinRight<'a, Rhs, F>
where
    F: Fn(AlgebraicValue) -> Result<IndexIter, DBError>,
    IndexIter: Iterator<Item = RowRef<'a>>,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        // Otherwise probe the index with a row from the probe side.
        let probe_col = self.probe_col.idx();
        while let Some(mut row) = self.probe_side.next() {
            if let Some(value) = row.read_or_take_column(probe_col) {
                let mut index_iter = (self.index_function)(value).expect(TABLE_ID_EXPECTED_VALID);
                while let Some(index_row) = index_iter.next().map(RelValue::Row) {
                    if self.filter(&index_row) {
                        return Some(row);
                    }
                }
            }
        }
        None
    }
}

/// Return an iterator index join operator that returns matching rows from the probe side.
pub fn index_semi_join_right<'a>(
    db: &'a RelationalDB,
    tx: &'a TxMode<'a>,
    probe_side: Box<IterRows<'a>>,
    probe_col: ColId,
    index_select: &'a Option<ColumnOp>,
    index_table: TableId,
    index_col: ColId,
) -> Box<IterRows<'a>> {
    match tx {
        TxMode::MutTx(tx) => Box::new(IndexSemiJoinRight {
            probe_side,
            probe_col,
            index_select,
            index_function: move |value| db.iter_by_col_range_mut(tx, index_table, index_col, value),
        }),
        TxMode::Tx(tx) => Box::new(IndexSemiJoinRight {
            probe_side,
            probe_col,
            index_select,
            index_function: move |value| db.iter_by_col_range(tx, index_table, index_col, value),
        }),
    }
}
static_assert_size!(
    IndexSemiJoinRight<
        Box<IterRows<'static>>,
        fn(AlgebraicValue) -> Result<IterByColRangeTx<'static, AlgebraicValue>, DBError>,
    >,
    40
);
static_assert_size!(
    IndexSemiJoinRight<
        Box<IterRows<'static>>,
        fn(AlgebraicValue) -> Result<IterByColRangeMutTx<'static, AlgebraicValue>, DBError>,
    >,
    40
);

/// A [ProgramVm] implementation that carry a [RelationalDB] for it
/// query execution
pub struct DbProgram<'db, 'tx> {
    pub(crate) db: &'db RelationalDB,
    pub(crate) tx: &'tx mut TxMode<'tx>,
    pub(crate) auth: AuthCtx,
}

/// If the subscriber is not the database owner,
/// reject the request if the estimated cardinality exceeds the limit.
pub fn check_row_limit<Query>(
    queries: &[Query],
    db: &RelationalDB,
    tx: &TxId,
    row_est: impl Fn(&Query, &TxId) -> u64,
    auth: &AuthCtx,
) -> Result<(), DBError> {
    if auth.caller != auth.owner {
        if let Some(limit) = StVarTable::row_limit(db, tx)? {
            let mut estimate: u64 = 0;
            for query in queries {
                estimate = estimate.saturating_add(row_est(query, tx));
            }
            if estimate > limit {
                return Err(DBError::Other(anyhow::anyhow!(
                    "Estimated cardinality ({estimate} rows) exceeds limit ({limit} rows)"
                )));
            }
        }
    }
    Ok(())
}

impl<'db, 'tx> DbProgram<'db, 'tx> {
    pub fn new(db: &'db RelationalDB, tx: &'tx mut TxMode<'tx>, auth: AuthCtx) -> Self {
        Self { db, tx, auth }
    }

    fn _eval_query<const N: usize>(&mut self, query: &QueryExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
        if let TxMode::Tx(tx) = self.tx {
            check_row_limit(
                &[query],
                self.db,
                tx,
                |expr, tx| estimation::num_rows(tx, expr),
                &self.auth,
            )?;
        }

        let table_access = query.source.table_access();
        tracing::trace!(table = query.source.table_name());

        let head = query.head().clone();
        let rows = build_query(self.db, self.tx, query, &mut |id| {
            sources.take(id).map(|mt| mt.into_iter().map(RelValue::Projection))
        })
        .collect_vec(|row| row.into_product_value());

        Ok(Code::Table(MemTable::new(head, table_access, rows)))
    }

    // TODO(centril): investigate taking bsatn as input instead.
    fn _execute_insert(&mut self, table: &DbTable, inserts: Vec<ProductValue>) -> Result<Code, ErrorVm> {
        let tx = self.tx.unwrap_mut();
        let mut scratch = Vec::new();
        for row in &inserts {
            row.encode(&mut scratch);
            self.db.insert(tx, table.table_id, &scratch)?;
            scratch.clear();
        }
        Ok(Code::Pass(Some(Update {
            table_id: table.table_id,
            table_name: table.head.table_name.clone(),
            inserts,
            deletes: Vec::default(),
        })))
    }

    fn _execute_update<const N: usize>(
        &mut self,
        delete: &QueryExpr,
        mut assigns: IntMap<ColId, ColExpr>,
        sources: Sources<'_, N>,
    ) -> Result<Code, ErrorVm> {
        let result = self._eval_query(delete, sources)?;
        let Code::Table(deleted) = result else {
            return Ok(result);
        };

        let table = delete
            .source
            .get_db_table()
            .expect("source for Update should be a DbTable");

        self._execute_delete(table, deleted.data.clone())?;

        // Replace the columns in the matched rows with the assigned
        // values. No typechecking is performed here, nor that all
        // assignments are consumed.
        let deletes = deleted.data.clone();
        let exprs: Vec<Option<ColExpr>> = (0..table.head.fields.len())
            .map(ColId::from)
            .map(|c| assigns.remove(&c))
            .collect();

        let insert_rows = deleted
            .data
            .into_iter()
            .map(|row| {
                let elements = row
                    .into_iter()
                    .zip(&exprs)
                    .map(|(val, expr)| {
                        if let Some(ColExpr::Value(assigned)) = expr {
                            assigned.clone()
                        } else {
                            val
                        }
                    })
                    .collect();

                ProductValue { elements }
            })
            .collect_vec();

        let result = self._execute_insert(table, insert_rows);
        let Ok(Code::Pass(Some(insert))) = result else {
            return result;
        };

        Ok(Code::Pass(Some(Update { deletes, ..insert })))
    }

    fn _execute_delete(&mut self, table: &DbTable, rows: Vec<ProductValue>) -> Result<Code, ErrorVm> {
        let deletes = rows.clone();
        self.db.delete_by_rel(self.tx.unwrap_mut(), table.table_id, rows);

        Ok(Code::Pass(Some(Update {
            table_id: table.table_id,
            table_name: table.head.table_name.clone(),
            inserts: Vec::default(),
            deletes,
        })))
    }

    fn _delete_query<const N: usize>(&mut self, query: &QueryExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
        match self._eval_query(query, sources)? {
            Code::Table(result) => self._execute_delete(query.source.get_db_table().unwrap(), result.data),
            r => Ok(r),
        }
    }

    fn _set_var(&mut self, name: String, literal: String) -> Result<Code, ErrorVm> {
        let tx = self.tx.unwrap_mut();
        StVarTable::write_var(self.db, tx, StVarName::from_str(&name)?, &literal)?;
        Ok(Code::Pass(None))
    }

    fn _read_var(&self, name: String) -> Result<Code, ErrorVm> {
        fn read_key_into_table(env: &DbProgram, name: &str) -> Result<MemTable, ErrorVm> {
            if let TxMode::Tx(tx) = &env.tx {
                let name = StVarName::from_str(name)?;
                if let Some(value) = StVarTable::read_var(env.db, tx, name)? {
                    return Ok(MemTable::from_iter(
                        Arc::new(st_var_schema().into()),
                        [ProductValue::from(StVarRow { name, value })],
                    ));
                }
            }
            Ok(MemTable::from_iter(Arc::new(st_var_schema().into()), []))
        }
        Ok(Code::Table(read_key_into_table(self, &name)?))
    }
}

impl ProgramVm for DbProgram<'_, '_> {
    // Safety: For DbProgram with tx = TxMode::Tx variant, all queries must match to CrudCode::Query and no other branch.
    fn eval_query<const N: usize>(&mut self, query: CrudExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
        query.check_auth(self.auth.owner, self.auth.caller)?;

        match query {
            CrudExpr::Query(query) => self._eval_query(&query, sources),
            CrudExpr::Insert { table, rows } => self._execute_insert(&table, rows),
            CrudExpr::Update { delete, assignments } => self._execute_update(&delete, assignments, sources),
            CrudExpr::Delete { query } => self._delete_query(&query, sources),
            CrudExpr::SetVar { name, literal } => self._set_var(name, literal),
            CrudExpr::ReadVar { name } => self._read_var(name),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{
        StColumnFields, StColumnRow, StFields as _, StIndexAlgorithm, StIndexFields, StIndexRow, StSequenceFields,
        StSequenceRow, StTableFields, StTableRow, ST_COLUMN_ID, ST_COLUMN_NAME, ST_INDEX_ID, ST_INDEX_NAME,
        ST_RESERVED_SEQUENCE_RANGE, ST_SEQUENCE_ID, ST_SEQUENCE_NAME, ST_TABLE_ID, ST_TABLE_NAME,
    };
    use crate::db::relational_db::tests_utils::{begin_tx, insert, with_auto_commit, with_read_only, TestDB};
    use pretty_assertions::assert_eq;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::relation::{FieldName, Header};
    use spacetimedb_sats::{product, AlgebraicType, ProductType, ProductValue};
    use spacetimedb_schema::def::{BTreeAlgorithm, IndexAlgorithm};
    use spacetimedb_schema::schema::{ColumnSchema, IndexSchema, TableSchema};
    use spacetimedb_vm::eval::run_ast;
    use spacetimedb_vm::eval::test_helpers::{mem_table, mem_table_one_u64, scalar};
    use spacetimedb_vm::operator::OpCmp;
    use std::sync::Arc;

    pub(crate) fn create_table_with_rows(
        db: &RelationalDB,
        tx: &mut MutTx,
        table_name: &str,
        schema: ProductType,
        rows: &[ProductValue],
        access: StAccess,
    ) -> ResultTest<Arc<TableSchema>> {
        let columns = schema
            .elements
            .iter()
            .enumerate()
            .map(|(i, element)| ColumnSchema {
                table_id: TableId::SENTINEL,
                col_name: element.name.as_ref().unwrap().clone(),
                col_type: element.algebraic_type.clone(),
                col_pos: ColId(i as _),
            })
            .collect();

        let table_id = db.create_table(
            tx,
            TableSchema::new(
                TableId::SENTINEL,
                table_name.into(),
                columns,
                vec![],
                vec![],
                vec![],
                StTableType::User,
                access,
                None,
                None,
            ),
        )?;
        let schema = db.schema_for_table_mut(tx, table_id)?;

        for row in rows {
            insert(db, tx, table_id, &row)?;
        }

        Ok(schema)
    }

    /// Creates a table "inventory" with `(inventory_id: u64, name : String)` as columns.
    fn create_inv_table(db: &RelationalDB, tx: &mut MutTx) -> ResultTest<(Arc<TableSchema>, ProductValue)> {
        let schema_ty = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");
        let schema = create_table_with_rows(db, tx, "inventory", schema_ty.clone(), &[row.clone()], StAccess::Public)?;
        Ok((schema, row))
    }

    fn run_query<const N: usize>(
        db: &RelationalDB,
        q: QueryExpr,
        sources: SourceSet<Vec<ProductValue>, N>,
    ) -> MemTable {
        with_read_only(db, |tx| {
            let mut tx_mode = (&*tx).into();
            let p = &mut DbProgram::new(db, &mut tx_mode, AuthCtx::for_testing());
            match run_ast(p, q.into(), sources) {
                Code::Table(x) => x,
                x => panic!("invalid result {x}"),
            }
        })
    }

    #[test]
    fn test_db_query_inner_join() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let (schema, _) = with_auto_commit(&stdb, |tx| create_inv_table(&stdb, tx))?;
        let table_id = schema.table_id;

        let data = mem_table_one_u64(u32::MAX.into());
        let mut sources = SourceSet::<_, 1>::empty();
        let rhs_source_expr = sources.add_mem_table(data);
        let q = QueryExpr::new(&*schema).with_join_inner(rhs_source_expr, 0.into(), 0.into(), false);
        let result = run_query(&stdb, q, sources);

        // The expected result.
        let inv = ProductType::from([AlgebraicType::U64, AlgebraicType::String, AlgebraicType::U64]);
        let row = product![1u64, "health", 1u64];
        let input = mem_table(table_id, inv, vec![row]);

        assert_eq!(result.data, input.data, "Inventory");

        Ok(())
    }

    #[test]
    fn test_db_query_semijoin() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let (schema, row) = with_auto_commit(&stdb, |tx| create_inv_table(&stdb, tx))?;

        let data = mem_table_one_u64(u32::MAX.into());
        let mut sources = SourceSet::<_, 1>::empty();
        let rhs_source_expr = sources.add_mem_table(data);
        let q = QueryExpr::new(&*schema).with_join_inner(rhs_source_expr, 0.into(), 0.into(), true);
        let result = run_query(&stdb, q, sources);

        // The expected result.
        let input = mem_table(schema.table_id, schema.get_row_type().clone(), vec![row]);
        assert_eq!(result.data, input.data, "Inventory");

        Ok(())
    }

    fn check_catalog(db: &RelationalDB, name: &str, row: ProductValue, q: QueryExpr, schema: &TableSchema) {
        let result = run_query(db, q, [].into());
        let input = MemTable::from_iter(Header::from(schema).into(), [row]);
        assert_eq!(result, input, "{}", name);
    }

    #[test]
    fn test_query_catalog_tables() -> ResultTest<()> {
        let stdb = TestDB::durable()?;
        let schema = &*stdb.schema_for_table(&begin_tx(&stdb), ST_TABLE_ID).unwrap();

        let q = QueryExpr::new(schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_TABLE_ID, StTableFields::TableName.into()),
                scalar(ST_TABLE_NAME),
            )
            .unwrap();
        let st_table_row = StTableRow {
            table_id: ST_TABLE_ID,
            table_name: ST_TABLE_NAME.into(),
            table_type: StTableType::System,
            table_access: StAccess::Public,
            table_primary_key: Some(StTableFields::TableId.into()),
        }
        .into();
        check_catalog(&stdb, ST_TABLE_NAME, st_table_row, q, schema);

        Ok(())
    }

    #[test]
    fn test_query_catalog_columns() -> ResultTest<()> {
        let stdb = TestDB::durable()?;
        let schema = &*stdb.schema_for_table(&begin_tx(&stdb), ST_COLUMN_ID).unwrap();

        let q = QueryExpr::new(schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_COLUMN_ID, StColumnFields::TableId.into()),
                scalar(ST_COLUMN_ID),
            )
            .unwrap()
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_COLUMN_ID, StColumnFields::ColPos.into()),
                scalar(StColumnFields::TableId as u16),
            )
            .unwrap();
        let st_column_row = StColumnRow {
            table_id: ST_COLUMN_ID,
            col_pos: StColumnFields::TableId.col_id(),
            col_name: StColumnFields::TableId.col_name(),
            col_type: AlgebraicType::U32.into(),
        }
        .into();
        check_catalog(&stdb, ST_COLUMN_NAME, st_column_row, q, schema);

        Ok(())
    }

    #[test]
    fn test_query_catalog_indexes() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let (schema, _) = with_auto_commit(&db, |tx| create_inv_table(&db, tx))?;
        let table_id = schema.table_id;
        let columns = ColList::from(ColId(0));
        let index_name = "idx_1";
        let is_unique = false;

        let index = IndexSchema {
            table_id,
            index_id: IndexId::SENTINEL,
            index_name: index_name.into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm {
                columns: columns.clone(),
            }),
        };
        let index_id = with_auto_commit(&db, |tx| db.create_index(tx, index, is_unique))?;

        let indexes_schema = &*db.schema_for_table(&begin_tx(&db), ST_INDEX_ID).unwrap();
        let q = QueryExpr::new(indexes_schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_INDEX_ID, StIndexFields::IndexName.into()),
                scalar(index_name),
            )
            .unwrap();

        let st_index_row = StIndexRow {
            index_id,
            index_name: index_name.into(),
            table_id,
            index_algorithm: StIndexAlgorithm::BTree { columns },
        }
        .into();
        check_catalog(&db, ST_INDEX_NAME, st_index_row, q, indexes_schema);

        Ok(())
    }

    #[test]
    fn test_query_catalog_sequences() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let schema = &*db.schema_for_table(&begin_tx(&db), ST_SEQUENCE_ID).unwrap();
        let q = QueryExpr::new(schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_SEQUENCE_ID, StSequenceFields::TableId.into()),
                scalar(ST_SEQUENCE_ID),
            )
            .unwrap();
        let st_sequence_row = StSequenceRow {
            sequence_id: 5.into(),
            sequence_name: "st_sequence_sequence_id_seq".into(),
            table_id: ST_SEQUENCE_ID,
            col_pos: 0.into(),
            increment: 1,
            start: ST_RESERVED_SEQUENCE_RANGE as i128 + 1,
            min_value: 1,
            max_value: i128::MAX,
            allocated: ST_RESERVED_SEQUENCE_RANGE as i128,
        }
        .into();
        check_catalog(&db, ST_SEQUENCE_NAME, st_sequence_row, q, schema);

        Ok(())
    }
}
