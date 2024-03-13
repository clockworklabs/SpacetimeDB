//! The [DbProgram] that execute arbitrary queries & code against the database.

use crate::db::cursor::{CatalogCursor, IndexCursor, TableCursor};
use crate::db::datastore::locking_tx_datastore::IterByColRange;
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::execution_context::ExecutionContext;
use core::ops::RangeBounds;
use itertools::Itertools;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Address;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::def::TableDef;
use spacetimedb_sats::relation::{DbTable, FieldExpr, FieldName, Header, Relation, RowCount};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::eval::IterRows;
use spacetimedb_vm::expr::*;
use spacetimedb_vm::iterators::RelIter;
use spacetimedb_vm::program::ProgramVm;
use spacetimedb_vm::rel_ops::RelOps;
use spacetimedb_vm::relation::{MemTable, RelValue, Table};
use std::sync::Arc;

pub enum TxMode<'a> {
    MutTx(&'a mut MutTx),
    Tx(&'a Tx),
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

//TODO: This is partially duplicated from the `vm` crate to avoid borrow checker issues
//and pull all that crate in core. Will be revisited after trait refactor
pub fn build_query<'a>(
    ctx: &'a ExecutionContext,
    stdb: &'a RelationalDB,
    tx: &'a TxMode,
    query: QueryCode,
    sources: &mut SourceSet,
) -> Result<Box<IterRows<'a>>, ErrorVm> {
    // We're incrementally building a query iterator by applying each operation in the `query.query`.
    // Most such operations will modify their parent, but certain operations (i.e. `IndexJoin`s)
    // are only valid as the first operation in the list,
    // and construct a new base query.
    //
    // Branches which use `result` will do `unwrap_or_else(|| get_table(ctx, stdb, tx, &query.table, sources))`
    // to get an `IterRows` defaulting to the `query.table`.
    //
    // Branches which do not use the `result` will assert that it is `None`,
    // i.e. that they are the first operator.
    //
    // TODO(bikeshedding): Avoid duplication of the ugly `result.take().map(...).unwrap_or_else(...)?` expr?
    // TODO(bikeshedding): Refactor `QueryCode` to separate `IndexJoin` from other `Query` variants,
    //   removing the need for this convoluted logic?
    let mut result = None;

    for op in query.query {
        result = Some(match op {
            Query::IndexScan(IndexScan { table, columns, bounds }) => {
                iter_by_col_range(ctx, stdb, tx, table, columns, bounds)?
            }
            Query::IndexJoin(IndexJoin {
                probe_side,
                probe_field,
                index_side,
                index_select,
                index_col,
                return_index_rows,
            }) => {
                if result.is_some() {
                    return Err(anyhow::anyhow!("Invalid query: `IndexJoin` must be the first operator").into());
                }
                let index_header = index_side.head().clone();
                let index_table = index_side.table_id().unwrap();
                let probe_side = build_query(ctx, stdb, tx, probe_side.into(), sources)?;
                Box::new(IndexSemiJoin {
                    ctx,
                    db: stdb,
                    tx,
                    probe_side,
                    probe_field,
                    index_header,
                    index_select,
                    index_table,
                    index_col,
                    index_iter: None,
                    return_index_rows,
                })
            }
            Query::Select(cmp) => {
                let result = result
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| get_table(ctx, stdb, tx, &query.table, sources))?;
                let header = result.head().clone();
                let iter = result.select(move |row| cmp.compare(row, &header));
                Box::new(iter)
            }
            Query::Project(cols, _) => {
                let result = result
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| get_table(ctx, stdb, tx, &query.table, sources))?;
                if cols.is_empty() {
                    result
                } else {
                    let header = result.head().clone();
                    let iter = result.project(cols, move |cols, row| {
                        Ok(RelValue::Projection(row.project_owned(cols, &header)?))
                    })?;
                    Box::new(iter)
                }
            }
            Query::JoinInner(join) => {
                let result = result
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| get_table(ctx, stdb, tx, &query.table, sources))?;
                let iter = join_inner(ctx, stdb, tx, result, join, false, sources)?;
                Box::new(iter)
            }
        })
    }

    result
        .map(Ok)
        .unwrap_or_else(|| get_table(ctx, stdb, tx, &query.table, sources))
}

fn join_inner<'a>(
    ctx: &'a ExecutionContext,
    db: &'a RelationalDB,
    tx: &'a TxMode,
    lhs: impl RelOps<'a> + 'a,
    rhs: JoinExpr,
    semi: bool,
    sources: &mut SourceSet,
) -> Result<impl RelOps<'a> + 'a, ErrorVm> {
    let col_lhs = FieldExpr::Name(rhs.col_lhs);
    let col_rhs = FieldExpr::Name(rhs.col_rhs);
    let key_lhs = [col_lhs.clone()];
    let key_rhs = [col_rhs.clone()];

    let rhs = build_query(ctx, db, tx, rhs.rhs.into(), sources)?;
    let key_lhs_header = lhs.head().clone();
    let key_rhs_header = rhs.head().clone();
    let col_lhs_header = lhs.head().clone();
    let col_rhs_header = rhs.head().clone();

    let header = if semi {
        col_lhs_header.clone()
    } else {
        Arc::new(col_lhs_header.extend(&col_rhs_header))
    };

    lhs.join_inner(
        rhs,
        header,
        move |row| Ok(row.project(&key_lhs, &key_lhs_header)?),
        move |row| Ok(row.project(&key_rhs, &key_rhs_header)?),
        move |l, r| {
            let l = l.get(&col_lhs, &col_lhs_header)?;
            let r = r.get(&col_rhs, &col_rhs_header)?;
            Ok(l == r)
        },
        move |l, r| {
            if semi {
                l
            } else {
                l.extend(r)
            }
        },
    )
}

/// Resolve `query` to a table iterator, either an [`IterRows`] or a [`TableCursor`].
///
/// If `query` refers to a `MemTable`, this will `Option::take` said `MemTable` out of `sources`,
/// leaving `None`.
/// This means that a query cannot refer to a `MemTable` multiple times,
/// nor can a `SourceSet` which contains a `MemTable` be reused.
/// This is because [`IterRows`] takes ownership of the `MemTable`.
///
/// On the other hand, if the `query` is a `DbTable`, `sources` is unused
/// and therefore unmodified.
fn get_table<'a>(
    ctx: &'a ExecutionContext,
    stdb: &'a RelationalDB,
    tx: &'a TxMode,
    query: &SourceExpr,
    sources: &mut SourceSet,
) -> Result<Box<dyn RelOps<'a> + 'a>, ErrorVm> {
    let head = query.head().clone();
    let row_count = query.row_count();
    Ok(match query {
        SourceExpr::MemTable { source_id, .. } => Box::new(RelIter::new(
            head,
            row_count,
            sources.take_mem_table(*source_id).expect("Unable to get MemTable"),
        )) as Box<IterRows<'_>>,
        SourceExpr::DbTable(x) => {
            let iter = match tx {
                TxMode::MutTx(tx) => stdb.iter_mut(ctx, tx, x.table_id)?,
                TxMode::Tx(tx) => stdb.iter(ctx, tx, x.table_id)?,
            };
            Box::new(TableCursor::new(x.clone(), iter)?) as Box<IterRows<'_>>
        }
    })
}

fn iter_by_col_range<'a>(
    ctx: &'a ExecutionContext,
    db: &'a RelationalDB,
    tx: &'a TxMode,
    table: DbTable,
    columns: ColList,
    range: impl RangeBounds<AlgebraicValue> + 'a,
) -> Result<Box<dyn RelOps<'a> + 'a>, ErrorVm> {
    let iter = match tx {
        TxMode::MutTx(tx) => db.iter_by_col_range_mut(ctx, tx, table.table_id, columns, range)?,
        TxMode::Tx(tx) => db.iter_by_col_range(ctx, tx, table.table_id, columns, range)?,
    };
    Ok(Box::new(IndexCursor::new(table, iter)?) as Box<IterRows<'_>>)
}

/// An index join operator that returns matching rows from the index side.
pub struct IndexSemiJoin<'a, Rhs: RelOps<'a>> {
    /// An iterator for the probe side.
    /// The values returned will be used to probe the index.
    pub probe_side: Rhs,
    /// The field whose value will be used to probe the index.
    pub probe_field: FieldName,
    /// The header for the index side of the join.
    pub index_header: Arc<Header>,
    /// An optional predicate to evaluate over the matching rows of the index.
    pub index_select: Option<ColumnOp>,
    /// The table id on which the index is defined.
    pub index_table: TableId,
    /// The column id for which the index is defined.
    pub index_col: ColId,
    /// Is this a left or right semijion?
    pub return_index_rows: bool,
    /// An iterator for the index side.
    /// A new iterator will be instantiated for each row on the probe side.
    pub index_iter: Option<IterByColRange<'a, AlgebraicValue>>,
    /// A reference to the database.
    pub db: &'a RelationalDB,
    /// A reference to the current transaction.
    pub tx: &'a TxMode<'a>,
    /// The execution context for the current transaction.
    ctx: &'a ExecutionContext<'a>,
}

impl<'a, Rhs: RelOps<'a>> IndexSemiJoin<'a, Rhs> {
    fn filter(&self, index_row: &RelValue<'_>) -> Result<bool, ErrorVm> {
        Ok(if let Some(op) = &self.index_select {
            op.compare(index_row, &self.index_header)?
        } else {
            true
        })
    }

    fn map(&self, index_row: RelValue<'a>, probe_row: Option<RelValue<'a>>) -> RelValue<'a> {
        if let Some(value) = probe_row {
            if !self.return_index_rows {
                return value;
            }
        }
        index_row
    }
}

impl<'a, Rhs: RelOps<'a>> RelOps<'a> for IndexSemiJoin<'a, Rhs> {
    fn head(&self) -> &Arc<Header> {
        if self.return_index_rows {
            &self.index_header
        } else {
            self.probe_side.head()
        }
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        // Return a value from the current index iterator, if not exhausted.
        if self.return_index_rows {
            while let Some(value) = self.index_iter.as_mut().and_then(|iter| iter.next()) {
                let value = RelValue::Row(value);
                if self.filter(&value)? {
                    return Ok(Some(self.map(value, None)));
                }
            }
        }

        // Otherwise probe the index with a row from the probe side.
        while let Some(row) = self.probe_side.next()? {
            if let Some(pos) = self.probe_side.head().column_pos(&self.probe_field) {
                if let Some(value) = row.read_column(pos.idx()) {
                    let table_id = self.index_table;
                    let col_id = self.index_col;

                    let value = value.into_owned();

                    let mut index_iter = match self.tx {
                        TxMode::MutTx(tx) => self.db.iter_by_col_range_mut(self.ctx, tx, table_id, col_id, value)?,
                        TxMode::Tx(tx) => self.db.iter_by_col_range(self.ctx, tx, table_id, col_id, value)?,
                    };
                    while let Some(value) = index_iter.next() {
                        let value = RelValue::Row(value);
                        if self.filter(&value)? {
                            self.index_iter = Some(index_iter);
                            return Ok(Some(self.map(value, Some(row))));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

/// A [ProgramVm] implementation that carry a [RelationalDB] for it
/// query execution
pub struct DbProgram<'db, 'tx> {
    ctx: &'tx ExecutionContext<'tx>,
    pub(crate) db: &'db RelationalDB,
    pub(crate) tx: &'tx mut TxMode<'tx>,
    pub(crate) auth: AuthCtx,
}

impl<'db, 'tx> DbProgram<'db, 'tx> {
    pub fn new(ctx: &'tx ExecutionContext, db: &'db RelationalDB, tx: &'tx mut TxMode<'tx>, auth: AuthCtx) -> Self {
        Self { ctx, db, tx, auth }
    }

    fn _eval_query(&mut self, query: QueryCode, sources: &mut SourceSet) -> Result<Code, ErrorVm> {
        let table_access = query.table.table_access();
        tracing::trace!(table = query.table.table_name());

        let result = build_query(self.ctx, self.db, self.tx, query, sources)?;
        let head = result.head().clone();
        let rows = result.collect_vec(|row| row.into_product_value())?;

        Ok(Code::Table(MemTable::new(head, table_access, rows)))
    }

    fn _execute_insert(&mut self, table: &Table, rows: Vec<ProductValue>) -> Result<Code, ErrorVm> {
        match self.tx {
            TxMode::MutTx(tx) => match table {
                // TODO: How do we deal with mutating values?
                Table::MemTable(_) => Err(ErrorVm::Other(anyhow::anyhow!("How deal with mutating values?"))),
                Table::DbTable(x) => {
                    for row in rows {
                        self.db.insert(tx, x.table_id, row)?;
                    }
                    Ok(Code::Pass)
                }
            },
            TxMode::Tx(_) => unreachable!("mutable operation is invalid with read tx"),
        }
    }

    fn _execute_delete(&mut self, table: &Table, rows: Vec<ProductValue>) -> Result<Code, ErrorVm> {
        match self.tx {
            TxMode::MutTx(tx) => match table {
                // TODO: How do we deal with mutating values?
                Table::MemTable(_) => Err(ErrorVm::Other(anyhow::anyhow!("How deal with mutating values?"))),
                Table::DbTable(t) => {
                    let count = self.db.delete_by_rel(tx, t.table_id, rows);
                    Ok(Code::Value(count.into()))
                }
            },
            TxMode::Tx(_) => unreachable!("mutable operation is invalid with read tx"),
        }
    }

    fn _delete_query(&mut self, query: QueryCode, sources: &mut SourceSet) -> Result<Code, ErrorVm> {
        let table = sources
            .take_table(&query.table)
            .expect("Cannot delete from a `MemTable`");
        let result = self._eval_query(query, sources)?;

        match result {
            Code::Table(result) => self._execute_delete(&table, result.data),
            _ => Ok(result),
        }
    }

    fn _create_table(&mut self, table: TableDef) -> Result<Code, ErrorVm> {
        match self.tx {
            TxMode::MutTx(tx) => {
                self.db.create_table(tx, table)?;
                Ok(Code::Pass)
            }
            TxMode::Tx(_) => unreachable!("mutable operation is invalid with read tx"),
        }
    }

    fn _drop(&mut self, name: &str, kind: DbType) -> Result<Code, ErrorVm> {
        match self.tx {
            TxMode::MutTx(tx) => {
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
                Ok(Code::Pass)
            }
            TxMode::Tx(_) => unreachable!("mutable operation is invalid with read tx"),
        }
    }
}

impl ProgramVm for DbProgram<'_, '_> {
    fn address(&self) -> Option<Address> {
        Some(self.db.address())
    }

    fn ctx(&self) -> &dyn ProgramVm {
        self as &dyn ProgramVm
    }

    fn auth(&self) -> &AuthCtx {
        &self.auth
    }

    // Safety: For DbProgram with tx = TxMode::Tx variant, all queries must match to CrudCode::Query and no other branch.
    fn eval_query(&mut self, query: CrudCode, sources: &mut SourceSet) -> Result<Code, ErrorVm> {
        query.check_auth(self.auth.owner, self.auth.caller)?;

        match query {
            CrudCode::Query(query) => self._eval_query(query, sources),
            CrudCode::Insert { table, rows } => {
                let src = sources.take_table(&table).unwrap();
                self._execute_insert(&src, rows)
            }
            CrudCode::Update {
                delete,
                mut assignments,
            } => {
                let table = delete.table.clone();
                let result = self._eval_query(delete, sources)?;

                let Code::Table(deleted) = result else {
                    return Ok(result);
                };
                let table = sources.take_table(&table).unwrap();
                self._execute_delete(&table, deleted.data.clone())?;

                // Replace the columns in the matched rows with the assigned
                // values. No typechecking is performed here, nor that all
                // assignments are consumed.
                let exprs: Vec<Option<FieldExpr>> = table
                    .head()
                    .fields
                    .iter()
                    .map(|col| assignments.remove(&col.field))
                    .collect();
                let insert_rows = deleted
                    .data
                    .into_iter()
                    .map(|row| {
                        let elements = row
                            .elements
                            .into_iter()
                            .zip(&exprs)
                            .map(|(val, expr)| {
                                if let Some(FieldExpr::Value(assigned)) = expr {
                                    assigned.clone()
                                } else {
                                    val
                                }
                            })
                            .collect();

                        ProductValue { elements }
                    })
                    .collect_vec();

                self._execute_insert(&table, insert_rows)
            }
            CrudCode::Delete { query } => {
                let result = self._delete_query(query, sources)?;
                Ok(result)
            }
            CrudCode::CreateTable { table } => {
                let result = self._create_table(table)?;
                Ok(result)
            }
            CrudCode::Drop {
                name,
                kind,
                table_access: _,
            } => {
                let result = self._drop(&name, kind)?;
                Ok(result)
            }
        }
    }
}

impl<'a> RelOps<'a> for TableCursor<'a> {
    fn head(&self) -> &Arc<Header> {
        &self.table.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        Ok(self.iter.next().map(RelValue::Row))
    }
}

impl<'a, R: RangeBounds<AlgebraicValue>> RelOps<'a> for IndexCursor<'a, R> {
    fn head(&self) -> &Arc<Header> {
        &self.table.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        Ok(self.iter.next().map(RelValue::Row))
    }
}

impl<'a, I> RelOps<'a> for CatalogCursor<I>
where
    I: Iterator<Item = ProductValue>,
{
    fn head(&self) -> &Arc<Header> {
        &self.table.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        if let Some(row) = self.iter.next() {
            return Ok(Some(RelValue::Projection(row)));
        };
        Ok(None)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{
        st_columns_schema, st_indexes_schema, st_sequences_schema, st_table_schema, StColumnFields, StColumnRow,
        StIndexFields, StIndexRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow, ST_COLUMNS_ID,
        ST_COLUMNS_NAME, ST_INDEXES_NAME, ST_SEQUENCES_ID, ST_SEQUENCES_NAME, ST_TABLES_ID, ST_TABLES_NAME,
    };
    use crate::db::datastore::traits::IsolationLevel;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::execution_context::ExecutionContext;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::{ColumnDef, IndexDef, IndexType};
    use spacetimedb_sats::relation::{DbTable, FieldName};
    use spacetimedb_sats::{product, AlgebraicType, ProductType, ProductValue};
    use spacetimedb_vm::dsl::*;
    use spacetimedb_vm::eval::run_ast;
    use spacetimedb_vm::operator::OpCmp;

    pub(crate) fn create_table_with_rows(
        db: &RelationalDB,
        tx: &mut MutTx,
        table_name: &str,
        schema: ProductType,
        rows: &[ProductValue],
    ) -> ResultTest<TableId> {
        let columns: Vec<_> = schema
            .elements
            .into_iter()
            .enumerate()
            .map(|(i, e)| ColumnDef {
                col_name: e.name.unwrap_or(i.to_string()),
                col_type: e.algebraic_type,
            })
            .collect();

        let table_id = db.create_table(
            tx,
            TableDef::new(table_name.into(), columns)
                .with_type(StTableType::User)
                .with_access(StAccess::for_name(table_name)),
        )?;
        for row in rows {
            db.insert(tx, table_id, row.clone())?;
        }

        Ok(table_id)
    }

    pub(crate) fn create_table_from_program(
        p: &mut DbProgram,
        table_name: &str,
        schema: ProductType,
        rows: &[ProductValue],
    ) -> ResultTest<TableId> {
        let db = &mut p.db;
        match p.tx {
            TxMode::MutTx(tx) => create_table_with_rows(db, tx, table_name, schema, rows),
            TxMode::Tx(_) => panic!("tx type should be mutable"),
        }
    }

    #[test]
    /// Inventory
    /// | inventory_id: u64 | name : String |
    /// Player
    /// | entity_id: u64 | inventory_id : u64 |
    /// Location
    /// | entity_id: u64 | x : f32 | z : f32 |
    fn test_db_query() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let tx_mode = &mut TxMode::MutTx(&mut tx);
        let p = &mut DbProgram::new(&ctx, &stdb, tx_mode, AuthCtx::for_testing());

        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");
        let table_id = create_table_from_program(p, "inventory", head.clone(), &[row])?;

        let schema = TableDef::from_product("test", head).into_schema(table_id);

        let data = MemTable::from_value(scalar(1u64));
        let rhs = data.get_field_pos(0).unwrap().clone();

        let mut sources = SourceSet::default();
        let rhs_source_expr = sources.add_mem_table(data);

        let q = query(&schema).with_join_inner(rhs_source_expr, FieldName::positional("inventory", 0), rhs);

        let result = match run_ast(p, q.into(), sources) {
            Code::Table(x) => x,
            x => panic!("invalid result {x}"),
        };

        //The expected result
        let inv = ProductType::from([
            (Some("inventory_id"), AlgebraicType::U64),
            (Some("name"), AlgebraicType::String),
            (None, AlgebraicType::U64),
        ]);
        let row = product!(scalar(1u64), scalar("health"), scalar(1u64));
        let input = mem_table(inv, vec![row]);

        assert_eq!(result.data, input.data, "Inventory");

        stdb.rollback_mut_tx(&ctx, tx);

        Ok(())
    }

    fn check_catalog(p: &mut DbProgram, name: &str, row: ProductValue, q: QueryExpr, schema: DbTable) {
        let result = run_ast(p, q.into(), SourceSet::default());

        //The expected result
        let input = mem_table(schema.head.clone_for_error(), vec![row]);

        assert_eq!(result, Code::Table(input), "{}", name);
    }

    #[test]
    fn test_query_catalog_tables() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let tx_mode = &mut TxMode::MutTx(&mut tx);
        let p = &mut DbProgram::new(&ctx, &stdb, tx_mode, AuthCtx::for_testing());

        let q = query(&st_table_schema()).with_select_cmp(
            OpCmp::Eq,
            FieldName::named(ST_TABLES_NAME, StTableFields::TableName.name()),
            scalar(ST_TABLES_NAME),
        );
        check_catalog(
            p,
            ST_TABLES_NAME,
            StTableRow {
                table_id: ST_TABLES_ID,
                table_name: ST_TABLES_NAME.to_string(),
                table_type: StTableType::System,
                table_access: StAccess::Public,
            }
            .into(),
            q,
            DbTable::from(&st_table_schema()),
        );

        stdb.rollback_mut_tx(&ctx, tx);

        Ok(())
    }

    #[test]
    fn test_query_catalog_columns() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let tx_mode = &mut TxMode::MutTx(&mut tx);
        let p = &mut DbProgram::new(&ctx, &stdb, tx_mode, AuthCtx::for_testing());

        let q = query(&st_columns_schema())
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::named(ST_COLUMNS_NAME, StColumnFields::TableId.name()),
                scalar(ST_COLUMNS_ID),
            )
            .with_select_cmp(
                OpCmp::Eq,
                FieldName::named(ST_COLUMNS_NAME, StColumnFields::ColPos.name()),
                scalar(StColumnFields::TableId as u32),
            );
        check_catalog(
            p,
            ST_COLUMNS_NAME,
            StColumnRow {
                table_id: ST_COLUMNS_ID,
                col_pos: StColumnFields::TableId.col_id(),
                col_name: StColumnFields::TableId.col_name(),
                col_type: AlgebraicType::U32,
            }
            .into(),
            q,
            (&st_columns_schema()).into(),
        );

        stdb.rollback_mut_tx(&ctx, tx);

        Ok(())
    }

    #[test]
    fn test_query_catalog_indexes() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;

        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let table_id = create_table_with_rows(&db, &mut tx, "inventory", head, &[row])?;
        db.commit_tx(&ctx, tx)?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let index = IndexDef::btree("idx_1".into(), ColId(0), true);
        let index_id = db.create_index(&mut tx, table_id, index)?;
        let tx_mode = &mut TxMode::MutTx(&mut tx);
        let p = &mut DbProgram::new(&ctx, &db, tx_mode, AuthCtx::for_testing());

        let q = query(&st_indexes_schema()).with_select_cmp(
            OpCmp::Eq,
            FieldName::named(ST_INDEXES_NAME, StIndexFields::IndexName.name()),
            scalar("idx_1"),
        );
        check_catalog(
            p,
            ST_INDEXES_NAME,
            StIndexRow {
                index_id,
                index_name: "idx_1".to_owned(),
                table_id,
                columns: ColList::new(0.into()),
                is_unique: true,
                index_type: IndexType::BTree,
            }
            .into(),
            q,
            (&st_indexes_schema()).into(),
        );

        db.rollback_mut_tx(&ctx, tx);

        Ok(())
    }

    #[test]
    fn test_query_catalog_sequences() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let tx_mode = &mut TxMode::MutTx(&mut tx);
        let p = &mut DbProgram::new(&ctx, &db, tx_mode, AuthCtx::for_testing());

        let q = query(&st_sequences_schema()).with_select_cmp(
            OpCmp::Eq,
            FieldName::named(ST_SEQUENCES_NAME, StSequenceFields::TableId.name()),
            scalar(ST_SEQUENCES_ID),
        );
        check_catalog(
            p,
            ST_SEQUENCES_NAME,
            StSequenceRow {
                sequence_id: 3.into(),
                sequence_name: "seq_st_sequence_sequence_id_primary_key_auto".to_string(),
                table_id: 2.into(),
                col_pos: 0.into(),
                increment: 1,
                start: 4,
                min_value: 1,
                max_value: i128::MAX,
                allocated: 4096,
            }
            .into(),
            q,
            (&st_sequences_schema()).into(),
        );

        db.rollback_mut_tx(&ctx, tx);

        Ok(())
    }
}
