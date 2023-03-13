//! The [Program] that execute arbitrary queries & code against the database.
use crate::db::catalog::CatalogKind;
use crate::db::cursor::{CatalogCursor, TableCursor};
use crate::db::relational_db::{RelationalDBWrapper, ST_COLUMNS_ID, ST_TABLES_ID};
use crate::error::DBError;
use spacetimedb_sats::relation::Relation;
use spacetimedb_sats::relation::{Header, MemTable, RelIter, RelValue, RowCount, Table};
use spacetimedb_sats::ProductValue;
use spacetimedb_vm::env::EnvDb;
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::eval::{build_query, IterRows};
use spacetimedb_vm::expr::*;
use spacetimedb_vm::program::{ProgramRef, ProgramVm};
use spacetimedb_vm::rel_ops::RelOps;
use std::collections::HashMap;

/// A [ProgramVm] implementation that carry a [RelationalDB] as context
/// for query execution
pub struct Program {
    pub(crate) env: EnvDb,
    pub(crate) stats: HashMap<String, u64>,
    pub(crate) db: RelationalDBWrapper,
}

impl Program {
    pub fn new(db: RelationalDBWrapper) -> Self {
        let mut env = EnvDb::new();
        Self::load_ops(&mut env);
        Self {
            env,
            db,
            stats: Default::default(),
        }
    }
}

impl ProgramVm for Program {
    fn env(&self) -> &EnvDb {
        &self.env
    }

    fn env_mut(&mut self) -> &mut EnvDb {
        &mut self.env
    }

    fn eval_query(&mut self, query: QueryCode) -> Result<Code, ErrorVm> {
        let mut db = self.db.lock().unwrap();
        let mut tx_ = db.begin_tx();
        let (tx, stdb) = tx_.get();

        let head = query.head();
        let row_count = query.row_count();
        let result = match query.data {
            Table::MemTable(x) => Box::new(RelIter::new(head, row_count, x)) as Box<IterRows<'_>>,
            Table::DbTable(x) => {
                let idx_id = stdb.catalog.indexes.table_idx_id;
                let seq_id = stdb.catalog.sequences.table_idx_id;

                match x.table_id {
                    ST_TABLES_ID => {
                        let row_count = RowCount::exact(stdb.catalog.tables.len());
                        let iter = stdb.catalog.tables.iter_row();
                        Box::new(CatalogCursor::new(
                            stdb.catalog.tables.schema_table(),
                            CatalogKind::Table,
                            row_count,
                            iter,
                        )) as Box<IterRows<'_>>
                    }
                    ST_COLUMNS_ID => {
                        let row_count = RowCount::exact(stdb.catalog.tables.len_columns());
                        let iter = stdb.catalog.tables.iter_columns_row();
                        Box::new(CatalogCursor::new(
                            stdb.catalog.tables.schema_columns(),
                            CatalogKind::Column,
                            row_count,
                            iter,
                        )) as Box<IterRows<'_>>
                    }
                    x if x == idx_id => {
                        let row_count = RowCount::exact(stdb.catalog.indexes.len());
                        let iter = stdb.catalog.indexes.iter_row();
                        Box::new(CatalogCursor::new(
                            stdb.catalog.indexes.schema(),
                            CatalogKind::Index,
                            row_count,
                            iter,
                        )) as Box<IterRows<'_>>
                    }
                    x if x == seq_id => {
                        let row_count = RowCount::exact(stdb.catalog.sequences.len());
                        let iter = stdb.catalog.sequences.iter_row();
                        Box::new(CatalogCursor::new(
                            stdb.catalog.sequences.schema(),
                            CatalogKind::Sequence,
                            row_count,
                            iter,
                        )) as Box<IterRows<'_>>
                    }
                    _ => {
                        let iter = stdb.scan(tx, x.table_id)?;

                        Box::new(TableCursor::new(x, iter)?) as Box<IterRows<'_>>
                    }
                }
            }
        };

        let result = build_query(result, query.query)?;
        let head = result.head().clone();
        let rows: Vec<_> = result.collect_vec()?;

        Ok(Code::Table(MemTable::new(&head, &rows)))
    }

    fn as_program_ref(&self) -> ProgramRef<'_> {
        ProgramRef {
            env: &self.env,
            stats: &self.stats,
        }
    }
}

impl RelOps for TableCursor<'_> {
    fn head(&self) -> &Header {
        &self.table.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }

    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        if let Some(row) = self.iter.next() {
            return Ok(Some(RelValue::new(self.head(), &row)));
        };
        Ok(None)
    }
}

impl From<DBError> for ErrorVm {
    fn from(err: DBError) -> Self {
        ErrorVm::Other(err.into())
    }
}

impl<I> RelOps for CatalogCursor<I>
where
    I: Iterator<Item = ProductValue>,
{
    fn head(&self) -> &Header {
        &self.table.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        if let Some(row) = self.iter.next() {
            return Ok(Some(RelValue::new(self.head(), &row)));
        };
        Ok(None)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::catalog::ST_SEQUENCE_SEQ;
    use crate::db::index::{IndexCatalog, IndexDef, IndexFields};
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::db::relational_db::{RelationalDB, ST_COLUMNS_NAME, ST_INDEXES_NAME, ST_TABLES_ID, ST_TABLES_NAME};
    use crate::db::sequence::{Sequence, SequenceCatalog, SequenceDef, SequenceFields};
    use crate::db::table::{ColumnFields, ColumnIndexAttribute, TableCatalog, TableFields};
    use crate::db::transactional_db::Tx;
    use spacetimedb_lib::error::{ResultTest, TestError};
    use spacetimedb_sats::relation::{DbTable, FieldName};
    use spacetimedb_sats::{product, AlgebraicType, BuiltinType, ProductType, ProductValue};
    use spacetimedb_vm::dsl::*;
    use spacetimedb_vm::eval::run_ast;
    use spacetimedb_vm::operator::OpCmp;

    pub(crate) fn create_table_with_rows(
        stdb: &mut RelationalDB,
        tx: &mut Tx,
        table_name: &str,
        schema: ProductType,
        rows: &[ProductValue],
    ) -> ResultTest<u32> {
        let table_id = stdb.create_table(tx, table_name, schema)?;

        for row in rows {
            stdb.insert(tx, table_id, row.clone())?;
        }

        Ok(table_id)
    }

    pub(crate) fn create_table_with_program(
        p: &mut Program,
        table_name: &str,
        schema: ProductType,
        rows: &[ProductValue],
    ) -> ResultTest<u32> {
        let mut db = p.db.lock().unwrap();
        let mut tx_ = db.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = create_table_with_rows(stdb, tx, table_name, schema, rows)?;
        tx_.commit()?;

        Ok(table_id)
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

        let p = &mut Program::new(RelationalDBWrapper::new(stdb));

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");
        let table_id = create_table_with_program(p, "inventory", head.clone(), &[row])?;

        let inv = db_table(head, table_id);

        let q = query(inv).with_join_inner(scalar(1u64), FieldName::Pos(0), FieldName::Pos(0));

        let result = run_ast(p, q.into());

        //The expected result
        let inv = ProductType::from_iter([
            (Some("inventory_id"), BuiltinType::U64),
            (Some("name"), BuiltinType::String),
            (None, BuiltinType::U64),
        ]);
        let row = product!(scalar(1u64), scalar("health"), scalar(1u64));
        let input = mem_table(inv, vec![row]);

        assert_eq!(result, Code::Table(input), "Inventory");

        Ok(())
    }

    fn check_catalog(p: &mut Program, name: &str, row: ProductValue, q: QueryExpr, schema: DbTable) {
        let result = run_ast(p, q.into());

        //The expected result
        let input = mem_table(schema.head.head, vec![row]);

        assert_eq!(result, Code::Table(input), "{}", name);
    }

    #[test]
    fn test_query_catalog_tables() -> ResultTest<()> {
        let (stdb, _) = make_test_db()?;
        let schema_tables = stdb.catalog.tables.schema_table();
        let p = &mut Program::new(RelationalDBWrapper::new(stdb));

        let q = query(schema_tables.clone()).with_select(
            OpCmp::Eq,
            FieldName::Name(TableFields::TableName.name().to_string()),
            scalar(ST_TABLES_NAME),
        );
        check_catalog(
            p,
            ST_TABLES_NAME,
            TableCatalog::make_row_table(ST_TABLES_ID, ST_TABLES_NAME, true),
            q,
            schema_tables,
        );

        Ok(())
    }

    #[test]
    fn test_query_catalog_columns() -> ResultTest<()> {
        let (stdb, _) = make_test_db()?;
        let schema_tables = stdb.catalog.tables.schema_columns();
        let p = &mut Program::new(RelationalDBWrapper::new(stdb));

        let q = query(schema_tables.clone())
            .with_select(
                OpCmp::Eq,
                FieldName::Name(ColumnFields::TableId.name().to_string()),
                scalar(ST_COLUMNS_ID),
            )
            .with_select(
                OpCmp::Eq,
                FieldName::Name(ColumnFields::ColId.name().to_string()),
                scalar(ColumnFields::TableId as u32),
            );
        check_catalog(
            p,
            ST_COLUMNS_NAME,
            TableCatalog::make_row_column(
                ST_COLUMNS_ID,
                ColumnFields::TableId as u32,
                ColumnFields::TableId.name(),
                &AlgebraicType::U32,
                ColumnIndexAttribute::UnSet,
            ),
            q,
            schema_tables,
        );

        Ok(())
    }

    #[test]
    fn test_query_catalog_indexes() -> ResultTest<()> {
        let (mut stdb, _) = make_test_db()?;
        let schema_indexes = stdb.catalog.indexes.schema();

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");

        let (table_id, index_id) = stdb.begin_tx().with(|tx, stdb| {
            let table_id = create_table_with_rows(stdb, tx, "inventory", head.clone(), &[row])?;

            let idx = IndexDef::new("idx_1", table_id, 0, true);

            let index_id = stdb.create_index(tx, idx)?;
            Ok::<_, TestError>((table_id, index_id))
        })?;

        let p = &mut Program::new(RelationalDBWrapper::new(stdb));

        let q = query(schema_indexes.clone()).with_select(
            OpCmp::Eq,
            FieldName::Name(IndexFields::IndexName.name().to_string()),
            scalar("idx_1"),
        );
        check_catalog(
            p,
            ST_INDEXES_NAME,
            IndexCatalog::make_row(index_id.0, "idx_1", table_id, 0, true, 1),
            q,
            schema_indexes,
        );

        Ok(())
    }

    #[test]
    fn test_query_catalog_sequences() -> ResultTest<()> {
        let (stdb, _) = make_test_db()?;
        let schema_sequences = stdb.catalog.sequences.schema();
        let seq_id = stdb.catalog.sequences.seq_id;

        let p = &mut Program::new(RelationalDBWrapper::new(stdb));

        let q = query(schema_sequences.clone()).with_select(
            OpCmp::Eq,
            FieldName::Name(SequenceFields::SequenceId.name().to_string()),
            scalar(seq_id.0),
        );
        let seq = Sequence::from_def(seq_id, SequenceDef::new(ST_SEQUENCE_SEQ))?;
        check_catalog(
            p,
            ST_SEQUENCE_SEQ,
            SequenceCatalog::make_row(
                seq.sequence_id.0,
                &seq.sequence_name,
                None,
                None,
                seq.current,
                seq.min_value,
                seq.max_value,
                seq.increment,
                seq.cache,
            ),
            q,
            schema_sequences,
        );

        Ok(())
    }
}
