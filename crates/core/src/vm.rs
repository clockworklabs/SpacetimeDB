//! The [DbProgram] that execute arbitrary queries & code against the database.

use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::datastore::locking_tx_datastore::IterByColRange;
use crate::db::datastore::system_tables::{st_var_schema, StVarName, StVarRow, StVarTable};
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation;
use crate::execution_context::ExecutionContext;
use core::ops::{Bound, RangeBounds};
use itertools::Itertools;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::def::TableDef;
use spacetimedb_sats::relation::{ColExpr, DbTable};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_table::static_assert_size;
use spacetimedb_table::table::RowRef;
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::eval::{build_project, build_select, join_inner, IterRows};
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

impl<'a> TxMode<'a> {
    /// Unwraps `self`, ensuring we are in a mutable tx.
    fn unwrap_mut(&mut self) -> &mut MutTx {
        match self {
            Self::MutTx(tx) => tx,
            Self::Tx(_) => unreachable!("mutable operation is invalid with read tx"),
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
    ctx: &'a ExecutionContext,
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
            .unwrap_or_else(|| get_table(ctx, db, tx, &query.source, sources))
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
                    iter_by_col_range(ctx, db, tx, table, columns.clone(), bounds)
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
                } else if cols.is_singleton() {
                    // For singleton constraints, we compare the column directly against `bounds`.
                    let head = cols.head().idx();
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
                let probe_side = build_query(ctx, db, tx, probe_side, sources);
                // The compiler guarantees that the index side is a db table,
                // and therefore this unwrap is always safe.
                let index_table = index_side.table_id().unwrap();

                if *return_index_rows {
                    Box::new(IndexSemiJoinLeft {
                        ctx,
                        db,
                        tx,
                        probe_side,
                        probe_col: *probe_col,
                        index_select,
                        index_table,
                        index_col: *index_col,
                        index_iter: None,
                    }) as Box<IterRows<'_>>
                } else {
                    Box::new(IndexSemiJoinRight {
                        ctx,
                        db,
                        tx,
                        probe_side,
                        probe_col: *probe_col,
                        index_select,
                        index_table,
                        index_col: *index_col,
                    })
                }
            }
            Query::Select(cmp) => build_select(result_or_base(sources, &mut result), cmp),
            Query::Project(proj) => build_project(result_or_base(sources, &mut result), proj),
            Query::JoinInner(join) => join_inner(
                result_or_base(sources, &mut result),
                build_query(ctx, db, tx, &join.rhs, sources),
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
    ctx: &'a ExecutionContext,
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
            TxMode::MutTx(tx) => stdb.iter_mut(ctx, tx, db_table.table_id),
            TxMode::Tx(tx) => stdb.iter(ctx, tx, db_table.table_id),
        }),
    }
}

fn iter_by_col_range<'a>(
    ctx: &'a ExecutionContext,
    db: &'a RelationalDB,
    tx: &'a TxMode,
    table: &'a DbTable,
    columns: ColList,
    range: impl RangeBounds<AlgebraicValue> + 'a,
) -> Box<IterRows<'a>> {
    build_iter_from_db(match tx {
        TxMode::MutTx(tx) => db.iter_by_col_range_mut(ctx, tx, table.table_id, columns, range),
        TxMode::Tx(tx) => db.iter_by_col_range(ctx, tx, table.table_id, columns, range),
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
pub struct IndexSemiJoinLeft<'a, 'c, Rhs: RelOps<'a>> {
    /// An iterator for the probe side.
    /// The values returned will be used to probe the index.
    pub probe_side: Rhs,
    /// The column whose value will be used to probe the index.
    pub probe_col: ColId,
    /// An optional predicate to evaluate over the matching rows of the index.
    pub index_select: &'c Option<ColumnOp>,
    /// The table id on which the index is defined.
    pub index_table: TableId,
    /// The column id for which the index is defined.
    pub index_col: ColId,
    /// An iterator for the index side.
    /// A new iterator will be instantiated for each row on the probe side.
    pub index_iter: Option<IterByColRange<'a, AlgebraicValue>>,
    /// A reference to the database.
    pub db: &'a RelationalDB,
    /// A reference to the current transaction.
    pub tx: &'a TxMode<'a>,
    /// The execution context for the current transaction.
    ctx: &'a ExecutionContext,
}

static_assert_size!(IndexSemiJoinLeft<Box<IterRows<'static>>>, 312);

impl<'a, Rhs: RelOps<'a>> IndexSemiJoinLeft<'a, '_, Rhs> {
    fn filter(&self, index_row: &RelValue<'_>) -> bool {
        self.index_select.as_ref().map_or(true, |op| op.eval_bool(index_row))
    }
}

impl<'a, Rhs: RelOps<'a>> RelOps<'a> for IndexSemiJoinLeft<'a, '_, Rhs> {
    fn next(&mut self) -> Option<RelValue<'a>> {
        // Return a value from the current index iterator, if not exhausted.
        while let Some(index_row) = self.index_iter.as_mut().and_then(|iter| iter.next()).map(RelValue::Row) {
            if self.filter(&index_row) {
                return Some(index_row);
            }
        }

        // Otherwise probe the index with a row from the probe side.
        let table_id = self.index_table;
        let index_col = self.index_col;
        let probe_col = self.probe_col.idx();
        while let Some(mut row) = self.probe_side.next() {
            if let Some(value) = row.read_or_take_column(probe_col) {
                let index_iter = match self.tx {
                    TxMode::MutTx(tx) => self.db.iter_by_col_range_mut(self.ctx, tx, table_id, index_col, value),
                    TxMode::Tx(tx) => self.db.iter_by_col_range(self.ctx, tx, table_id, index_col, value),
                };
                let mut index_iter = index_iter.expect(TABLE_ID_EXPECTED_VALID);
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

/// An index join operator that returns matching rows from the probe side.
pub struct IndexSemiJoinRight<'a, 'c, Rhs: RelOps<'a>> {
    /// An iterator for the probe side.
    /// The values returned will be used to probe the index.
    pub probe_side: Rhs,
    /// The column whose value will be used to probe the index.
    pub probe_col: ColId,
    /// An optional predicate to evaluate over the matching rows of the index.
    pub index_select: &'c Option<ColumnOp>,
    /// The table id on which the index is defined.
    pub index_table: TableId,
    /// The column id for which the index is defined.
    pub index_col: ColId,
    /// A reference to the database.
    pub db: &'a RelationalDB,
    /// A reference to the current transaction.
    pub tx: &'a TxMode<'a>,
    /// The execution context for the current transaction.
    ctx: &'a ExecutionContext,
}

static_assert_size!(IndexSemiJoinRight<Box<IterRows<'static>>>, 64);

impl<'a, Rhs: RelOps<'a>> IndexSemiJoinRight<'a, '_, Rhs> {
    fn filter(&self, index_row: &RelValue<'_>) -> bool {
        self.index_select.as_ref().map_or(true, |op| op.eval_bool(index_row))
    }
}

impl<'a, Rhs: RelOps<'a>> RelOps<'a> for IndexSemiJoinRight<'a, '_, Rhs> {
    fn next(&mut self) -> Option<RelValue<'a>> {
        // Otherwise probe the index with a row from the probe side.
        let table_id = self.index_table;
        let index_col = self.index_col;
        let probe_col = self.probe_col.idx();
        while let Some(row) = self.probe_side.next() {
            if let Some(value) = row.read_column(probe_col) {
                let value = &*value;
                let index_iter = match self.tx {
                    TxMode::MutTx(tx) => self.db.iter_by_col_range_mut(self.ctx, tx, table_id, index_col, value),
                    TxMode::Tx(tx) => self.db.iter_by_col_range(self.ctx, tx, table_id, index_col, value),
                };
                let mut index_iter = index_iter.expect(TABLE_ID_EXPECTED_VALID);
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

/// A [ProgramVm] implementation that carry a [RelationalDB] for it
/// query execution
pub struct DbProgram<'db, 'tx> {
    pub(crate) ctx: &'tx ExecutionContext,
    pub(crate) db: &'db RelationalDB,
    pub(crate) tx: &'tx mut TxMode<'tx>,
    pub(crate) auth: AuthCtx,
}

/// If the subscriber is not the database owner,
/// reject the request if the estimated cardinality exceeds the limit.
pub fn check_row_limit<QuerySet>(
    queries: &QuerySet,
    ctx: &ExecutionContext,
    db: &RelationalDB,
    tx: &TxId,
    row_est: impl Fn(&QuerySet, &TxId) -> u64,
    auth: &AuthCtx,
) -> Result<(), DBError> {
    if auth.caller != auth.owner {
        if let Some(limit) = StVarTable::row_limit(ctx, db, tx)? {
            let estimate = row_est(queries, tx);
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
    pub fn new(ctx: &'tx ExecutionContext, db: &'db RelationalDB, tx: &'tx mut TxMode<'tx>, auth: AuthCtx) -> Self {
        Self { ctx, db, tx, auth }
    }

    fn _eval_query<const N: usize>(&mut self, query: &QueryExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
        if let TxMode::Tx(tx) = self.tx {
            check_row_limit(
                query,
                self.ctx,
                self.db,
                tx,
                |expr, tx| estimation::num_rows(tx, expr),
                &self.auth,
            )?;
        }

        let table_access = query.source.table_access();
        tracing::trace!(table = query.source.table_name());

        let head = query.head().clone();
        let rows = build_query(self.ctx, self.db, self.tx, query, &mut |id| {
            sources.take(id).map(|mt| mt.into_iter().map(RelValue::Projection))
        })
        .collect_vec(|row| row.into_product_value());

        Ok(Code::Table(MemTable::new(head, table_access, rows)))
    }

    fn _execute_insert(&mut self, table: &DbTable, rows: Vec<ProductValue>) -> Result<Code, ErrorVm> {
        let tx = self.tx.unwrap_mut();
        let inserts = rows.clone(); // TODO code shouldn't be hot, let's remove later
        for row in rows {
            self.db.insert(tx, table.table_id, row)?;
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

    fn _create_table(&mut self, table: TableDef) -> Result<Code, ErrorVm> {
        self.db.create_table(self.tx.unwrap_mut(), table)?;
        Ok(Code::Pass(None))
    }

    fn _drop(&mut self, name: &str, kind: DbType) -> Result<Code, ErrorVm> {
        let tx = self.tx.unwrap_mut();

        match kind {
            DbType::Table => {
                if let Some(id) = self.db.table_id_from_name_mut(tx, name)? {
                    self.db.drop_table(self.ctx, tx, id)?;
                }
            }
            DbType::Index => {
                if let Some(id) = self.db.index_id_from_name(tx, name)? {
                    self.db.drop_index(tx, id)?;
                }
            }
            DbType::Sequence => {
                if let Some(id) = self.db.sequence_id_from_name(tx, name)? {
                    self.db.drop_sequence(tx, id)?;
                }
            }
            DbType::Constraint => {
                if let Some(id) = self.db.constraint_id_from_name(tx, name)? {
                    self.db.drop_constraint(tx, id)?;
                }
            }
        }
        Ok(Code::Pass(None))
    }

    fn _set_var(&mut self, name: String, literal: String) -> Result<Code, ErrorVm> {
        let tx = self.tx.unwrap_mut();
        StVarTable::write_var(self.ctx, self.db, tx, StVarName::from_str(&name)?, &literal)?;
        Ok(Code::Pass(None))
    }

    fn _read_var(&self, name: String) -> Result<Code, ErrorVm> {
        fn read_key_into_table(env: &DbProgram, name: &str) -> Result<MemTable, ErrorVm> {
            if let TxMode::Tx(tx) = &env.tx {
                let name = StVarName::from_str(name)?;
                if let Some(value) = StVarTable::read_var(env.ctx, env.db, tx, name)? {
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
            CrudExpr::CreateTable { table } => self._create_table(*table),
            CrudExpr::Drop { name, kind, .. } => self._drop(&name, kind),
            CrudExpr::SetVar { name, literal } => self._set_var(name, literal),
            CrudExpr::ReadVar { name } => self._read_var(name),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{
        StColumnFields, StColumnRow, StFields as _, StIndexFields, StIndexRow, StSequenceFields, StSequenceRow,
        StTableFields, StTableRow, ST_COLUMNS_ID, ST_COLUMNS_NAME, ST_INDEXES_ID, ST_INDEXES_NAME,
        ST_RESERVED_SEQUENCE_RANGE, ST_SEQUENCES_ID, ST_SEQUENCES_NAME, ST_TABLES_ID, ST_TABLES_NAME,
    };
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::execution_context::ExecutionContext;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::{ColumnDef, IndexDef, IndexType, TableSchema};
    use spacetimedb_sats::relation::{FieldName, Header};
    use spacetimedb_sats::{product, AlgebraicType, ProductType, ProductValue};
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
        let columns: Vec<_> = Vec::from(schema.elements)
            .into_iter()
            .enumerate()
            .map(|(i, e)| ColumnDef {
                col_name: e.name.unwrap_or_else(|| i.to_string().into()),
                col_type: e.algebraic_type,
            })
            .collect();

        let table_id = db.create_table(
            tx,
            TableDef::new(table_name.into(), columns)
                .with_type(StTableType::User)
                .with_access(access),
        )?;
        let schema = db.schema_for_table_mut(tx, table_id)?;

        for row in rows {
            db.insert(tx, table_id, row.clone())?;
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
        let ctx = ExecutionContext::default();
        db.with_read_only(&ctx, |tx| {
            let mut tx_mode = (&*tx).into();
            let p = &mut DbProgram::new(&ctx, db, &mut tx_mode, AuthCtx::for_testing());
            match run_ast(p, q.into(), sources) {
                Code::Table(x) => x,
                x => panic!("invalid result {x}"),
            }
        })
    }

    #[test]
    fn test_db_query_inner_join() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let (schema, _) = stdb.with_auto_commit(&ExecutionContext::default(), |tx| create_inv_table(&stdb, tx))?;
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

        let ctx = ExecutionContext::default();
        let (schema, row) = stdb.with_auto_commit(&ctx, |tx| create_inv_table(&stdb, tx))?;

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
        let schema = &*stdb.schema_for_table(&stdb.begin_tx(), ST_TABLES_ID).unwrap();

        let q = QueryExpr::new(schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_TABLES_ID, StTableFields::TableName.into()),
                scalar(ST_TABLES_NAME),
            )
            .unwrap();
        let st_table_row = StTableRow {
            table_id: ST_TABLES_ID,
            table_name: ST_TABLES_NAME.into(),
            table_type: StTableType::System,
            table_access: StAccess::Public,
        }
        .into();
        check_catalog(&stdb, ST_TABLES_NAME, st_table_row, q, schema);

        Ok(())
    }

    #[test]
    fn test_query_catalog_columns() -> ResultTest<()> {
        let stdb = TestDB::durable()?;
        let schema = &*stdb.schema_for_table(&stdb.begin_tx(), ST_COLUMNS_ID).unwrap();

        let q = QueryExpr::new(schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_COLUMNS_ID, StColumnFields::TableId.into()),
                scalar(ST_COLUMNS_ID),
            )
            .unwrap()
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_COLUMNS_ID, StColumnFields::ColPos.into()),
                scalar(StColumnFields::TableId as u32),
            )
            .unwrap();
        let st_column_row = StColumnRow {
            table_id: ST_COLUMNS_ID,
            col_pos: StColumnFields::TableId.col_id(),
            col_name: StColumnFields::TableId.col_name(),
            col_type: AlgebraicType::U32,
        }
        .into();
        check_catalog(&stdb, ST_COLUMNS_NAME, st_column_row, q, schema);

        Ok(())
    }

    #[test]
    fn test_query_catalog_indexes() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let ctx = ExecutionContext::default();
        let (schema, _) = db.with_auto_commit(&ctx, |tx| create_inv_table(&db, tx))?;
        let table_id = schema.table_id;

        let index = IndexDef::btree("idx_1".into(), ColId(0), true);
        let index_id = db.with_auto_commit(&ctx, |tx| db.create_index(tx, table_id, index))?;

        let indexes_schema = &*db.schema_for_table(&db.begin_tx(), ST_INDEXES_ID).unwrap();
        let q = QueryExpr::new(indexes_schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_INDEXES_ID, StIndexFields::IndexName.into()),
                scalar("idx_1"),
            )
            .unwrap();
        let st_index_row = StIndexRow {
            index_id,
            index_name: "idx_1".into(),
            table_id,
            columns: ColList::new(0.into()),
            is_unique: true,
            index_type: IndexType::BTree,
        }
        .into();
        check_catalog(&db, ST_INDEXES_NAME, st_index_row, q, indexes_schema);

        Ok(())
    }

    #[test]
    fn test_query_catalog_sequences() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let schema = &*db.schema_for_table(&db.begin_tx(), ST_SEQUENCES_ID).unwrap();
        let q = QueryExpr::new(schema)
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::new(ST_SEQUENCES_ID, StSequenceFields::TableId.into()),
                scalar(ST_SEQUENCES_ID),
            )
            .unwrap();
        let st_sequence_row = StSequenceRow {
            sequence_id: 3.into(),
            sequence_name: "seq_st_sequence_sequence_id_primary_key_auto".into(),
            table_id: 2.into(),
            col_pos: 0.into(),
            increment: 1,
            start: ST_RESERVED_SEQUENCE_RANGE as i128 + 1,
            min_value: 1,
            max_value: i128::MAX,
            allocated: ST_RESERVED_SEQUENCE_RANGE as i128 * 2,
        }
        .into();
        check_catalog(&db, ST_SEQUENCES_NAME, st_sequence_row, q, schema);

        Ok(())
    }
}
