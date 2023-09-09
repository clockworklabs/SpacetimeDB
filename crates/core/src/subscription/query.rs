use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, SubscriptionError};
use crate::host::module_host::DatabaseTableUpdate;
use crate::sql::compiler::compile_sql;
use crate::sql::execute::execute_single_sql;
use crate::subscription::subscription::QuerySet;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::{Column, FieldName, MemTable, RelValue};
use spacetimedb_lib::DataKey;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_vm::expr::{Crud, CrudExpr, DbType, QueryExpr, SourceExpr};

pub const SUBSCRIBE_TO_ALL_QUERY: &str = "SELECT * FROM *";

pub enum QueryDef {
    Table(String),
    Sql(String),
}

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Query {
    pub queries: Vec<QueryExpr>,
}

impl Query {
    #[tracing::instrument(skip(self))]
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

pub const OP_TYPE_FIELD_NAME: &str = "__op_type";

//HACK: To recover the `op_type` of this particular row I add a "hidden" column `OP_TYPE_FIELD_NAME`
#[tracing::instrument(skip_all)]
pub fn to_mem_table(of: QueryExpr, data: &DatabaseTableUpdate) -> QueryExpr {
    let mut q = of;
    let table_access = q.source.table_access();

    let mut t = match &q.source {
        SourceExpr::MemTable(x) => MemTable::new(&x.head, table_access, &[]),
        SourceExpr::DbTable(table) => MemTable::new(&table.head, table_access, &[]),
    };

    if let Some(pos) = t.head.find_pos_by_name(OP_TYPE_FIELD_NAME) {
        t.data.extend(data.ops.iter().map(|row| {
            let mut new = row.row.clone();
            new.elements[pos] = row.op_type.into();
            let mut bytes: &[u8] = row.row_pk.as_ref();
            RelValue::new(new, Some(DataKey::decode(&mut bytes).unwrap()))
        }));
    } else {
        t.head.fields.push(Column::new(
            FieldName::named(&t.head.table_name, OP_TYPE_FIELD_NAME),
            AlgebraicType::U8,
        ));
        for row in &data.ops {
            let mut new = row.row.clone();
            new.elements.push(row.op_type.into());
            let mut bytes: &[u8] = row.row_pk.as_ref();
            t.data
                .push(RelValue::new(new, Some(DataKey::decode(&mut bytes).unwrap())));
        }
    }

    q.source = SourceExpr::MemTable(t);

    q
}

/// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
#[tracing::instrument(skip_all)]
pub(crate) fn run_query(
    db: &RelationalDB,
    tx: &mut MutTxId,
    query: &QueryExpr,
    auth: AuthCtx,
) -> Result<Vec<MemTable>, DBError> {
    execute_single_sql(db, tx, CrudExpr::Query(query.clone()), auth)
}

// TODO: It's semantically wrong to `SUBSCRIBE_TO_ALL_QUERY`
// as it can only return back the changes valid for the tables in scope *right now*
// instead of **continuously updating** the db changes
// with system table modifications (add/remove tables, indexes, ...).
/// Compile from `SQL` into a [`Query`].
///
/// NOTE: When the `input` query is equal to [`SUBSCRIBE_TO_ALL_QUERY`],
/// **compilation is bypassed** and the equivalent of the following is done:
///
///```rust,ignore
/// for t in db.user_tables {
///   query.push(format!("SELECT * FROM {t}"));
/// }
/// ```
///
/// WARNING: [`SUBSCRIBE_TO_ALL_QUERY`] is only valid for repeated calls as long there is not change on database schema, and the clients must `unsubscribe` before modifying it.
#[tracing::instrument(skip(relational_db, auth, tx))]
pub fn compile_query(
    relational_db: &RelationalDB,
    tx: &MutTxId,
    auth: &AuthCtx,
    input: &str,
) -> Result<Query, DBError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SubscriptionError::Empty.into());
    }

    if input == SUBSCRIBE_TO_ALL_QUERY {
        return QuerySet::get_all(relational_db, tx, auth);
    }

    let compiled = compile_sql(relational_db, tx, input)?;
    let mut queries = Vec::with_capacity(compiled.len());
    for q in compiled {
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
    use crate::db::datastore::traits::TableSchema;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp};
    use crate::sql::execute::run;
    use crate::subscription::subscription::QuerySet;
    use crate::vm::tests::create_table_with_rows;
    use itertools::Itertools;
    use spacetimedb_lib::auth::{StAccess, StTableType};
    use spacetimedb_lib::data_key::ToDataKey;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::relation::FieldName;
    use spacetimedb_lib::Identity;
    use spacetimedb_sats::{product, BuiltinType, ProductType, ProductValue};
    use spacetimedb_vm::dsl::{db_table, mem_table, scalar};
    use spacetimedb_vm::operator::OpCmp;

    fn make_data(
        db: &RelationalDB,
        tx: &mut MutTxId,
        table_name: &str,
        head: &ProductType,
        row: &ProductValue,
    ) -> ResultTest<(TableSchema, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table = mem_table(head.clone(), [row.clone()]);
        let table_id = create_table_with_rows(db, tx, table_name, head.clone(), &[row.clone()])?;

        let schema = db.schema_for_table(tx, table_id).unwrap();

        let op = TableOp {
            op_type: 1,
            row_pk: row.to_data_key().to_bytes(),
            row: row.clone(),
        };

        let data = DatabaseTableUpdate {
            table_id,
            table_name: table_name.to_string(),
            ops: vec![op],
        };

        let q = QueryExpr::new(db_table((&schema).into(), table_name, table_id));

        Ok((schema, table, data, q))
    }

    fn make_inv(
        db: &RelationalDB,
        tx: &mut MutTxId,
        access: StAccess,
    ) -> ResultTest<(TableSchema, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table_name = if access == StAccess::Public {
            "inventory"
        } else {
            "_inventory"
        };

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row)?;

        // For filtering out the hidden field `OP_TYPE_FIELD_NAME`
        let fields = &[
            FieldName::named(table_name, "inventory_id").into(),
            FieldName::named(table_name, "name").into(),
        ];

        let q = q.with_project(fields);

        Ok((schema, table, data, q))
    }

    fn make_player(
        db: &RelationalDB,
        tx: &mut MutTxId,
    ) -> ResultTest<(TableSchema, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table_name = "player";
        let head = ProductType::from_iter([("player_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(2u64, "jhon doe");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row)?;

        // For filtering out the hidden field `OP_TYPE_FIELD_NAME`
        let fields = &[
            FieldName::named(table_name, "player_id").into(),
            FieldName::named(table_name, "name").into(),
        ];

        let q = q.with_project(fields);

        Ok((schema, table, data, q))
    }

    fn check_query(
        db: &RelationalDB,
        table: &MemTable,
        tx: &mut MutTxId,
        q: &QueryExpr,
        data: &DatabaseTableUpdate,
    ) -> ResultTest<()> {
        let q = to_mem_table(q.clone(), data);
        let result = run_query(db, tx, &q, AuthCtx::for_testing())?;

        assert_eq!(
            Some(table.as_without_table_name()),
            result.first().map(|x| x.as_without_table_name())
        );

        Ok(())
    }

    fn get_result(result: DatabaseUpdate) -> Vec<ProductValue> {
        result
            .tables
            .iter()
            .flat_map(|x| x.ops.iter().map(|x| x.row.clone()))
            .sorted()
            .collect::<Vec<_>>()
    }

    fn check_query_incr(
        db: &RelationalDB,
        tx: &mut MutTxId,
        s: &QuerySet,
        update: &DatabaseUpdate,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let result = s.eval_incr(db, tx, update, AuthCtx::for_testing())?;
        assert_eq!(
            result.tables.len(),
            total_tables,
            "Must return the correct number of tables"
        );

        let result = get_result(result);

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    fn check_query_eval(
        db: &RelationalDB,
        tx: &mut MutTxId,
        s: &QuerySet,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let result = s.eval(db, tx, AuthCtx::for_testing())?;
        assert_eq!(
            result.tables.len(),
            total_tables,
            "Must return the correct number of tables"
        );

        let result = get_result(result);

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    #[test]
    fn test_eval_incr_maintains_row_ids() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        let schema = ProductType::from_iter([("u8", BuiltinType::U8)]);
        let row = product!(1u8);

        // generate row id from row
        let id1 = &row.to_data_key().to_bytes();

        // create table empty table "test"
        let table_id = create_table_with_rows(&db, &mut tx, "test", schema.clone(), &[])?;

        // select * from test
        let query = QueryExpr::new(db_table(schema.clone(), "test", table_id));
        let query = QuerySet(vec![Query { queries: vec![query] }]);

        let op = TableOp {
            op_type: 0,
            row_pk: id1.clone(),
            row: row.clone(),
        };

        let update = DatabaseTableUpdate {
            table_id,
            table_name: "test".into(),
            ops: vec![op],
        };

        let update = DatabaseUpdate { tables: vec![update] };

        let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
        let id2 = &result.tables[0].ops[0].row_pk;

        // check that both row ids are the same
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Public)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Public);

        let q_1 = q.clone();
        check_query(&db, &table, &mut tx, &q_1, &data)?;

        let q_2 = q.with_select_cmp(OpCmp::Eq, FieldName::named("inventory", "inventory_id"), scalar(1u64));
        check_query(&db, &table, &mut tx, &q_2, &data)?;

        Ok(())
    }

    // Check that the `owner` can access private tables (that start with `_`) and that it fails if the `caller` is different
    #[test]
    fn test_subscribe_private() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Private)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Private);

        let row = product!(1u64, "health");
        check_query(&db, &table, &mut tx, &q, &data)?;

        //SELECT * FROM inventory
        let q_all = QueryExpr::new(db_table((&schema).into(), "_inventory", schema.table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("_inventory", "inventory_id"), scalar(1u64));

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
            table_id: schema.table_id,
            table_name: "_inventory".to_string(),
            ops: vec![row1, row2],
        };

        let update = DatabaseUpdate {
            tables: vec![data.clone()],
        };

        check_query_incr(&db, &mut tx, &s, &update, 3, &[row])?;

        let q = QueryExpr::new(db_table((&schema).into(), "_inventory", schema.table_id));

        let q = to_mem_table(q, &data);
        //Try access the private table
        match run_query(
            &db,
            &mut tx,
            &q,
            AuthCtx::new(Identity::__dummy(), Identity::from_byte_array([1u8; 32])),
        ) {
            Ok(_) => {
                panic!("it allows to execute against private table")
            }
            Err(err) => {
                if err.get_auth_error().is_none() {
                    panic!("fail to report an `auth` violation for private table, it gets {err}")
                }
            }
        }

        Ok(())
    }

    //Check that
    //```
    //SELECT * FROM table
    //SELECT * FROM table WHERE id=1
    //```
    // return just one row for both incr & direct subscriptions
    #[test]
    fn test_subscribe_dedup() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        let (schema, _table, _data, _q) = make_inv(&db, &mut tx, StAccess::Private)?;

        //SELECT * FROM inventory
        let q_all = QueryExpr::new(db_table((&schema).into(), "inventory", schema.table_id));
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

        check_query_eval(&db, &mut tx, &s, 3, &[product!(1u64, "health")])?;

        let row = product!(1u64, "health");

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
            table_id: schema.table_id,
            table_name: "inventory".to_string(),
            ops: vec![row1, row2],
        };

        let update = DatabaseUpdate { tables: vec![data] };

        check_query_incr(&db, &mut tx, &s, &update, 3, &[row])?;

        Ok(())
    }

    //Check that
    //```
    //SELECT * FROM table1
    //SELECT * FROM table2
    // =
    //SELECT * FROM table2
    //SELECT * FROM table1
    //```
    // return just one row irrespective of the order of the queries
    #[test]
    fn test_subscribe_commutative() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        let (_, _, _, q_1) = make_inv(&db, &mut tx, StAccess::Public)?;
        let (_, _, _, q_2) = make_player(&db, &mut tx)?;

        let s = QuerySet(vec![
            Query {
                queries: vec![q_1.clone()],
            },
            Query {
                queries: vec![q_2.clone()],
            },
        ]);

        let result_1 = s.eval(&db, &mut tx, AuthCtx::for_testing())?;

        let s = QuerySet(vec![Query { queries: vec![q_2] }, Query { queries: vec![q_1] }]);

        let result_2 = s.eval(&db, &mut tx, AuthCtx::for_testing())?;

        assert_eq!(get_result(result_1), get_result(result_2));

        Ok(())
    }

    #[test]
    fn test_subscribe_sql() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        let sql_create = "CREATE TABLE MobileEntityState (entity_id BIGINT UNSIGNED, location_x INTEGER, location_z INTEGER, destination_x INTEGER, destination_z INTEGER, is_running BOOLEAN, timestamp  BIGINT UNSIGNED, dimension INTEGER UNSIGNED);\
        CREATE TABLE EnemyState (entity_id BIGINT UNSIGNED, herd_id INTEGER, status INTEGER, type INTEGER, direction INTEGER);";
        run(&db, &mut tx, sql_create, AuthCtx::for_testing())?;

        let sql_create = "\
        insert into MobileEntityState (entity_id, location_x, location_z, destination_x, destination_z, is_running, timestamp, dimension) values (1, 96001, 96001, 96001, 1867045146, false, 17167179743690094247, 3926297397);\
        insert into MobileEntityState (entity_id, location_x, location_z, destination_x, destination_z, is_running, timestamp, dimension) values (2, 96001, 191000, 191000, 1560020888, true, 2947537077064292621, 445019304);

        insert into EnemyState (entity_id, herd_id, status, type, direction) values (1, 1181485940, 1633678837, 1158301365, 132191327);
        insert into EnemyState (entity_id, herd_id, status, type, direction) values (2, 2017368418, 194072456, 34423057, 1296770410);";
        run(&db, &mut tx, sql_create, AuthCtx::for_testing())?;

        let sql_query = "SELECT * FROM MobileEntityState JOIN EnemyState ON MobileEntityState.entity_id = EnemyState.entity_id WHERE location_x > 96000 AND MobileEntityState.location_x < 192000 AND MobileEntityState.location_z > 96000 AND MobileEntityState.location_z < 192000";
        let q = compile_query(&db, &tx, &AuthCtx::for_testing(), sql_query)?;

        for q in q.queries {
            assert_eq!(
                run_query(&db, &mut tx, &q, AuthCtx::for_testing())?.len(),
                1,
                "Not return results"
            );
        }

        Ok(())
    }

    #[test]
    fn test_subscribe_all() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        let (schema_1, _, _, _) = make_inv(&db, &mut tx, StAccess::Public)?;
        let (schema_2, _, _, _) = make_player(&db, &mut tx)?;
        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");

        let s = QuerySet(vec![compile_query(
            &db,
            &tx,
            &AuthCtx::for_testing(),
            SUBSCRIBE_TO_ALL_QUERY,
        )?]);

        check_query_eval(&db, &mut tx, &s, 2, &[row_1.clone(), row_2.clone()])?;

        let row1 = TableOp {
            op_type: 0,
            row_pk: row_1.to_data_key().to_bytes(),
            row: row_1,
        };

        let row2 = TableOp {
            op_type: 1,
            row_pk: row_2.to_data_key().to_bytes(),
            row: row_2,
        };

        let data1 = DatabaseTableUpdate {
            table_id: schema_1.table_id,
            table_name: "inventory".to_string(),
            ops: vec![row1],
        };

        let data2 = DatabaseTableUpdate {
            table_id: schema_2.table_id,
            table_name: "player".to_string(),
            ops: vec![row2],
        };

        let update = DatabaseUpdate {
            tables: vec![data1, data2],
        };

        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");
        check_query_incr(&db, &mut tx, &s, &update, 2, &[row_1, row_2])?;

        Ok(())
    }
}
