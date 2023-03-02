//! The [Program] that execute arbitrary queries & code against the database.
use crate::db::cursor::TableCursor;
use crate::db::relational_db::RelationalDBWrapper;
use crate::error::DBError;
use spacetimedb_sats::relation::Relation;
use spacetimedb_sats::relation::{Header, MemTable, RelIter, RelValue, RowCount, Table};
use spacetimedb_vm::env::EnvDb;
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::eval::{build_query, IterRows};
use spacetimedb_vm::expr::*;
use spacetimedb_vm::program::{ProgramRef, ProgramVm};
use spacetimedb_vm::rel_ops::RelOps;
use std::collections::HashMap;

/// A [ProgramVm] implementation that carry a [RelationalDB] for it
/// query execution
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
                let iter = stdb.scan(tx, x.table_id)?;

                Box::new(TableCursor::new(x, iter)?) as Box<IterRows<'_>>
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::relation::FieldName;
    use spacetimedb_sats::{product, BuiltinType, ProductType, ProductValue};
    use spacetimedb_vm::dsl::*;
    use spacetimedb_vm::eval::run_ast;

    pub(crate) fn create_table_with_rows(
        p: &mut Program,
        table_name: &str,
        schema: ProductType,
        rows: &[ProductValue],
    ) -> ResultTest<u32> {
        let mut db = p.db.lock().unwrap();
        let mut tx_ = db.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(tx, table_name, schema)?;

        for row in rows {
            stdb.insert(tx, table_id, row.clone())?;
        }
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
        let table_id = create_table_with_rows(p, "inventory", head.clone(), &[row])?;

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
}
