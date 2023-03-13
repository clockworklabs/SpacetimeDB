use crate::db::relational_db::RelationalDBWrapper;
use crate::db::table::TableDef;
use crate::error::{DBError, TableError};
use crate::host::module_host::DatabaseTableUpdate;
use crate::vm::Program;
use spacetimedb_sats::relation::MemTable;
use spacetimedb_vm::dsl::{mem_table, query};
use spacetimedb_vm::errors::{ErrorKind, ErrorUser};
use spacetimedb_vm::eval::run_ast;
use spacetimedb_vm::expr::Code;

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Query {
    pub table: TableDef,
}

/// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
pub(crate) fn run_query(
    db: RelationalDBWrapper,
    schema: &TableDef,
    table: &DatabaseTableUpdate,
) -> Result<MemTable, DBError> {
    let table = mem_table(schema.columns.columns.clone(), table.ops.iter().map(|x| x.row.clone()));
    let code = query(table);

    let mut p = Program::new(db);
    let code = run_ast(&mut p, code.into());

    match code {
        Code::Table(x) => Ok(x),
        Code::Halt(err) => Err(err.into()),
        x => Err(ErrorUser::new(
            ErrorKind::Invalid,
            Some(&format!("The query evaluate to {x} instead of a mem_table")),
        )
        .into()),
    }
}

pub fn compile_query(relational_db: &mut RelationalDBWrapper, input: &str) -> Result<Query, DBError> {
    let mut stdb = relational_db.lock().unwrap();

    let mut tx_ = stdb.begin_tx();
    let (_, stdb) = tx_.get();

    let result = stdb
        .scan_tables()
        .find_map(|(_, x)| if x.name == input { Some(x.clone()) } else { None });

    tx_.rollback();

    if let Some(table) = result {
        Ok(Query { table })
    } else {
        Err(TableError::NotFound(input.into()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::host::module_host::TableOp;
    use crate::vm::tests::create_table_with_program;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::{product, BuiltinType, ProductType};

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;
        let db = RelationalDBWrapper::new(stdb);
        let p = &mut Program::new(db.clone());

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");
        let table = mem_table(head.clone(), [row.clone()]);
        let table_id = create_table_with_program(p, "inventory", head, &[row.clone()])?;

        let stdb = p.db.lock().unwrap();

        let schema = stdb.catalog.tables.get_by_name("inventory").unwrap().clone();
        drop(stdb);

        let op = TableOp {
            op_type: 0,
            row_pk: vec![],
            row,
        };

        let data = DatabaseTableUpdate {
            table_id,
            table_name: "inventory".to_string(),
            ops: vec![op],
        };

        let result = run_query(db, &schema, &data)?;

        assert_eq!(table, result);
        Ok(())
    }
}
