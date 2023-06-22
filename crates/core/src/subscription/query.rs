use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, SubscriptionError};
use crate::host::module_host::DatabaseTableUpdate;
use crate::sql::execute::{compile_sql, execute_single_sql};
use spacetimedb_sats::relation::MemTable;
use spacetimedb_vm::expr::{Crud, CrudExpr, DbType, QueryExpr, SourceExpr};

pub enum QueryDef {
    Table(String),
    Sql(String),
}

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Query {
    pub queries: Vec<QueryExpr>,
}

impl Query {
    pub fn queries_of_table_id<'a>(&'a self, table: &'a DatabaseTableUpdate) -> impl Iterator<Item = QueryExpr> + '_ {
        self.queries.iter().filter_map(move |x| {
            if x.source.get_db_table().map(|x| x.table_id) == Some(table.table_id) {
                let t = to_mem_table(x.clone(), table);
                Some(t)
            } else {
                None
            }
        })
    }
}

pub fn to_mem_table(of: QueryExpr, data: &DatabaseTableUpdate) -> QueryExpr {
    let mut q = of;

    let mut t = match &q.source {
        SourceExpr::MemTable(x) => MemTable::new(&x.head, &[]),
        SourceExpr::DbTable(table) => MemTable::new(&table.head, &[]),
    };

    for row in &data.ops {
        t.data.push(row.row.clone());
    }

    q.source = SourceExpr::MemTable(t);

    q
}

/// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
pub(crate) fn run_query(db: &RelationalDB, query: &QueryExpr) -> Result<Vec<MemTable>, DBError> {
    execute_single_sql(db, CrudExpr::Query(query.clone()))
}

pub fn compile_query(relational_db: &RelationalDB, input: &str) -> Result<Query, DBError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SubscriptionError::Empty.into());
    }

    let mut queries = Vec::new();
    for q in compile_sql(relational_db, input)? {
        match q {
            CrudExpr::Query(x) => queries.push(x),
            CrudExpr::Insert { .. } => {
                return Err(SubscriptionError::SideEffect(Crud::Insert).into());
            }
            CrudExpr::Update { .. } => return Err(SubscriptionError::SideEffect(Crud::Update).into()),
            CrudExpr::Delete { .. } => return Err(SubscriptionError::SideEffect(Crud::Delete).into()),
            CrudExpr::CreateTable { .. } => {
                return Err(SubscriptionError::SideEffect(Crud::Create(DbType::Table)).into())
            }
            CrudExpr::Drop { kind, .. } => return Err(SubscriptionError::SideEffect(Crud::Drop(kind)).into()),
        }
    }

    if !queries.is_empty() {
        Ok(Query { queries })
    } else {
        Err(SubscriptionError::Empty.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp};
    use crate::subscription::subscription::QuerySet;
    use crate::vm::tests::create_table_from_program;
    use crate::vm::DbProgram;
    use spacetimedb_lib::data_key::ToDataKey;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::relation::FieldName;
    use spacetimedb_sats::{product, BuiltinType, ProductType, ProductValue};
    use spacetimedb_vm::dsl::{db_table, mem_table, scalar};
    use spacetimedb_vm::operator::OpCmp;

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;

        let mut tx = db.begin_tx();
        let p = &mut DbProgram::new(&db, &mut tx);

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");
        let table = mem_table(head.clone(), [row.clone()]);
        let table_id = create_table_from_program(p, "inventory", head.clone(), &[row.clone()])?;

        let schema = db.schema_for_table(&tx, table_id).unwrap();
        db.commit_tx(tx)?;

        let op = TableOp {
            op_type: 0,
            row_pk: vec![],
            row,
        };

        let data = DatabaseTableUpdate {
            table_id,
            table_name: "inventory".to_string(),
            ops: vec![op.clone()],
        };
        let q = QueryExpr::new(db_table((&schema).into(), "inventory", table_id));

        let q = to_mem_table(q, &data);
        let result = run_query(&db, &q)?;

        assert_eq!(
            Some(table.as_without_table_name()),
            result.first().map(|x| x.as_without_table_name())
        );

        let data = DatabaseTableUpdate {
            table_id,
            table_name: "inventory".to_string(),
            ops: vec![op],
        };

        let q = QueryExpr::new(db_table((&schema).into(), "inventory", table_id)).with_select_cmp(
            OpCmp::Eq,
            FieldName::named("inventory", "inventory_id"),
            scalar(0),
        );

        let q = to_mem_table(q, &data);
        let result = run_query(&db, &q)?;

        let table = mem_table(head, Vec::<ProductValue>::new());
        assert_eq!(
            Some(table.as_without_table_name()),
            result.first().map(|x| x.as_without_table_name())
        );

        Ok(())
    }

    //Check that
    //```
    //SELECT * FROM table
    //SELECT * FROM table WHERE id=1
    //```
    // return just one row
    #[test]
    fn test_subscribe_dedup() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;

        let mut tx = db.begin_tx();
        let p = &mut DbProgram::new(&db, &mut tx);

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");
        let table_id = create_table_from_program(p, "inventory", head, &[row.clone()])?;

        let schema = db.schema_for_table(&tx, table_id).unwrap();
        db.commit_tx(tx)?;

        //SELECT * FROM inventory
        let q_all = QueryExpr::new(db_table((&schema).into(), "inventory", table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("inventory", "inventory_id"), scalar(1u64));

        let s = QuerySet(vec![
            Query {
                queries: vec![q_all.clone()],
            },
            Query {
                queries: vec![q_all, q_id],
            },
        ]);

        let result = s.eval(&db)?;
        assert_eq!(result.tables.len(), 1, "Must return 1 table");
        assert_eq!(result.tables[0].ops.len(), 1, "Must return 1 row");
        assert_eq!(result.tables[0].ops[0].row, row, "Must return the correct row");

        Ok(())
    }

    #[test]
    fn test_subscribe_dedup_incr() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;

        let mut tx = db.begin_tx();
        let p = &mut DbProgram::new(&db, &mut tx);

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");
        let table_id = create_table_from_program(p, "inventory", head, &[row.clone()])?;

        let schema = db.schema_for_table(&tx, table_id).unwrap();
        db.commit_tx(tx)?;

        //SELECT * FROM inventory
        let q_all = QueryExpr::new(db_table((&schema).into(), "inventory", table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("inventory", "inventory_id"), scalar(1u64));

        let s = QuerySet(vec![
            Query {
                queries: vec![q_all.clone()],
            },
            Query {
                queries: vec![q_all, q_id],
            },
        ]);

        let row1 = TableOp {
            op_type: 0,
            row_pk: row.to_data_key().to_bytes(),
            row: row.clone(),
        };

        let row2 = TableOp {
            op_type: 1,
            row_pk: row.to_data_key().to_bytes(),
            row: row.clone(),
        };

        let data = DatabaseTableUpdate {
            table_id,
            table_name: "inventory".to_string(),
            ops: vec![row1, row2],
        };

        let update = DatabaseUpdate { tables: vec![data] };

        let result = s.eval_incr(&db, &update)?;
        assert_eq!(result.tables.len(), 1, "Must return 1 table");
        assert_eq!(result.tables[0].ops.len(), 1, "Must return 1 row");
        assert_eq!(result.tables[0].ops[0].row, row, "Must return the correct row");

        Ok(())
    }
}
