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
use spacetimedb_vm::expr::{self, Crud, CrudExpr, DbType, QueryExpr, SourceExpr};

pub const SUBSCRIBE_TO_ALL_QUERY: &str = "SELECT * FROM *";

pub enum QueryDef {
    Table(String),
    Sql(String),
}

pub const OP_TYPE_FIELD_NAME: &str = "__op_type";

//HACK: To recover the `op_type` of this particular row I add a "hidden" column `OP_TYPE_FIELD_NAME`
#[tracing::instrument(skip_all)]
pub fn to_mem_table(of: QueryExpr, data: &DatabaseTableUpdate) -> QueryExpr {
    let mut q = of;
    let table_access = q.source.table_access();

    let head = match &q.source {
        SourceExpr::MemTable(x) => &x.head,
        SourceExpr::DbTable(table) => &table.head,
    };
    let mut t = MemTable::new(head.clone(), table_access, vec![]);

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
/// Compile from `SQL` into a [`Query`], rejecting empty queries and queries that attempt to modify the data in any way.
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
pub fn compile_read_only_query(
    relational_db: &RelationalDB,
    tx: &MutTxId,
    auth: &AuthCtx,
    input: &str,
) -> Result<QuerySet, DBError> {
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
        Ok(queries.into_iter().collect())
    } else {
        Err(SubscriptionError::Empty.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Supported {
    Scan,
    Semijoin,
}

pub fn classify(expr: &QueryExpr) -> Option<Supported> {
    use expr::Query::*;
    if expr.query.len() == 1 && matches!(expr.query[0], IndexJoin(_)) {
        return Some(Supported::Semijoin);
    }
    for op in &expr.query {
        if let JoinInner(_) = op {
            return None;
        }
    }
    Some(Supported::Scan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::datastore::traits::{ColumnDef, IndexDef, TableDef, TableSchema};
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp};
    use crate::sql::execute::run;
    use crate::subscription::subscription::QuerySet;
    use crate::vm::tests::create_table_with_rows;
    use itertools::Itertools;
    use nonempty::NonEmpty;
    use spacetimedb_lib::auth::{StAccess, StTableType};
    use spacetimedb_lib::data_key::ToDataKey;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::relation::FieldName;
    use spacetimedb_lib::Identity;
    use spacetimedb_sats::{product, ProductType, ProductValue};
    use spacetimedb_vm::dsl::{db_table, mem_table, scalar};
    use spacetimedb_vm::operator::OpCmp;

    fn create_table(
        db: &RelationalDB,
        tx: &mut MutTxId,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(u32, &str)],
    ) -> ResultTest<u32> {
        let table_name = name.to_string();
        let table_type = StTableType::User;
        let table_access = StAccess::Public;

        let columns = schema
            .iter()
            .map(|(col_name, col_type)| ColumnDef {
                col_name: col_name.to_string(),
                col_type: col_type.clone(),
                is_autoinc: false,
            })
            .collect_vec();

        let indexes = indexes
            .iter()
            .map(|(col_id, index_name)| IndexDef {
                table_id: 0,
                cols: NonEmpty::new(*col_id),
                name: index_name.to_string(),
                is_unique: false,
            })
            .collect_vec();

        let schema = TableDef {
            table_name,
            columns,
            indexes,
            table_type,
            table_access,
        };

        Ok(db.create_table(tx, schema)?)
    }

    fn insert_op(table_id: u32, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
        let row_pk = row.to_data_key().to_bytes();
        DatabaseTableUpdate {
            table_id,
            table_name: table_name.to_string(),
            ops: vec![TableOp {
                op_type: 1,
                row,
                row_pk,
            }],
        }
    }

    fn delete_op(table_id: u32, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
        let row_pk = row.to_data_key().to_bytes();
        DatabaseTableUpdate {
            table_id,
            table_name: table_name.to_string(),
            ops: vec![TableOp {
                op_type: 0,
                row,
                row_pk,
            }],
        }
    }

    fn insert_row(db: &RelationalDB, tx: &mut MutTxId, table_id: u32, row: ProductValue) -> ResultTest<()> {
        db.insert(tx, table_id, row)?;
        Ok(())
    }

    fn delete_row(db: &RelationalDB, tx: &mut MutTxId, table_id: u32, row: ProductValue) -> ResultTest<()> {
        db.delete_by_rel(tx, table_id, vec![row])?;
        Ok(())
    }

    fn make_data(
        db: &RelationalDB,
        tx: &mut MutTxId,
        table_name: &str,
        head: &ProductType,
        row: &ProductValue,
    ) -> ResultTest<(TableSchema, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table = mem_table(head.clone(), [row.clone()]);
        let table_id = create_table_with_rows(db, tx, table_name, head.clone(), &[row.clone()])?;

        let schema = db.schema_for_table(tx, table_id).unwrap().into_owned();

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

        let q = QueryExpr::new(db_table((&schema).into(), table_name.to_owned(), table_id));

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

        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row)?;

        // For filtering out the hidden field `OP_TYPE_FIELD_NAME`
        let fields = &[
            FieldName::named(table_name, "inventory_id").into(),
            FieldName::named(table_name, "name").into(),
        ];

        let q = q.with_project(fields, None);

        Ok((schema, table, data, q))
    }

    fn make_player(
        db: &RelationalDB,
        tx: &mut MutTxId,
    ) -> ResultTest<(TableSchema, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table_name = "player";
        let head = ProductType::from([("player_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(2u64, "jhon doe");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row)?;

        // For filtering out the hidden field `OP_TYPE_FIELD_NAME`
        let fields = &[
            FieldName::named(table_name, "player_id").into(),
            FieldName::named(table_name, "name").into(),
        ];

        let q = q.with_project(fields, None);

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
            "Must return the correct number of tables: {result:#?}"
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
            "Must return the correct number of tables: {result:#?}"
        );

        let result = get_result(result);

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    #[test]
    fn test_eval_incr_maintains_row_ids() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        let schema = ProductType::from([("u8", AlgebraicType::U8)]);
        let row = product!(1u8);

        // generate row id from row
        let id1 = &row.to_data_key().to_bytes();

        // create table empty table "test"
        let table_id = create_table_with_rows(&db, &mut tx, "test", schema.clone(), &[])?;

        // select * from test
        let query: QuerySet = QueryExpr::new(db_table(schema.clone(), "test".to_string(), table_id)).into();

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
    fn test_eval_incr_for_index_scan() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1, "b")];
        let table_id = create_table(&db, &mut tx, "test", schema, indexes)?;

        let sql = "select * from test where b = 3";
        let mut exp = compile_sql(&db, &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query = QuerySet::from(query);

        let mut ops = Vec::new();
        for i in 0u64..9u64 {
            let row = product!(i, i);
            db.insert(&mut tx, table_id, row)?;

            let row = product!(i + 10, i);
            let row_pk = row.to_data_key().to_bytes();
            ops.push(TableOp {
                op_type: 0,
                row_pk,
                row,
            })
        }

        let update = DatabaseUpdate {
            tables: vec![DatabaseTableUpdate {
                table_id,
                table_name: "test".into(),
                ops,
            }],
        };

        let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

        assert_eq!(result.tables.len(), 1);

        let update = &result.tables[0];

        assert_eq!(update.ops.len(), 1);

        let op = &update.ops[0];

        assert_eq!(op.op_type, 0);
        assert_eq!(op.row, product!(13u64, 3u64));
        assert_eq!(op.row_pk, product!(13u64, 3u64).to_data_key().to_bytes());
        Ok(())
    }

    #[test]
    fn test_eval_incr_for_index_join() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [lhs] with index on [id]
        let schema = &[("id", AlgebraicType::I32), ("x", AlgebraicType::I32)];
        let indexes = &[(0, "id")];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with no indexes
        let schema = &[
            ("rid", AlgebraicType::I32),
            ("id", AlgebraicType::I32),
            ("y", AlgebraicType::I32),
        ];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, &[])?;

        // Insert into lhs
        for i in 0..5 {
            db.insert(&mut tx, lhs_id, product!(i, i + 5))?;
        }

        // Insert into rhs
        for i in 10..20 {
            db.insert(&mut tx, rhs_id, product!(i, i - 10, i - 8))?;
        }

        // Should be answered using an index semijion
        let sql = "select lhs.* from lhs join rhs on lhs.id = rhs.id where rhs.y >= 2 and rhs.y <= 4";
        let mut exp = compile_sql(&db, &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query = QuerySet::from(query);

        // Case 1: Delete a row inside the region and insert back inside the region
        {
            let r1 = product!(10, 0, 2);
            let r2 = product!(10, 0, 3);

            delete_row(&db, &mut tx, rhs_id, r1.clone())?;
            insert_row(&db, &mut tx, rhs_id, r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            insert_row(&db, &mut tx, rhs_id, r1.clone())?;
            delete_row(&db, &mut tx, rhs_id, r2.clone())?;
        }

        // Case 2: Delete a row outside the region and insert back outside the region
        {
            let r1 = product!(13, 3, 5);
            let r2 = product!(13, 3, 6);

            insert_row(&db, &mut tx, rhs_id, r1.clone())?;
            delete_row(&db, &mut tx, rhs_id, r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            insert_row(&db, &mut tx, rhs_id, r1.clone())?;
            delete_row(&db, &mut tx, rhs_id, r2.clone())?;
        }

        // Case 3: Delete a row inside the region and insert back outside the region
        {
            let r1 = product!(10, 0, 2);
            let r2 = product!(10, 0, 5);

            delete_row(&db, &mut tx, rhs_id, r1.clone())?;
            insert_row(&db, &mut tx, rhs_id, r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // A single delete from lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));

            // Clean up tx
            insert_row(&db, &mut tx, rhs_id, r1.clone())?;
            delete_row(&db, &mut tx, rhs_id, r2.clone())?;
        }

        // Case 4: Delete a row outside the region and insert back inside the region
        {
            let r1 = product!(13, 3, 5);
            let r2 = product!(13, 3, 4);

            delete_row(&db, &mut tx, rhs_id, r1.clone())?;
            insert_row(&db, &mut tx, rhs_id, r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // A single insert into lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(3, 8)));

            // Clean up tx
            insert_row(&db, &mut tx, rhs_id, r1.clone())?;
            delete_row(&db, &mut tx, rhs_id, r2.clone())?;
        }

        // Case 5: Insert a row into lhs and insert a matching row inside the region of rhs
        {
            let lhs_row = product!(5, 10);
            let rhs_row = product!(20, 5, 3);

            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;

            let updates = vec![
                insert_op(lhs_id, "lhs", lhs_row.clone()),
                insert_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // A single insert into lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(5, 10)));

            // Clean up tx
            delete_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
        }

        // Case 6: Insert a row into lhs and insert a matching row outside the region of rhs
        {
            let lhs_row = product!(5, 10);
            let rhs_row = product!(20, 5, 5);

            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;

            let updates = vec![
                insert_op(lhs_id, "lhs", lhs_row.clone()),
                insert_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            delete_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
        }

        // Case 7: Delete a row from lhs and delete a matching row inside the region of rhs
        {
            let lhs_row = product!(0, 5);
            let rhs_row = product!(10, 0, 2);

            delete_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone())?;

            let updates = vec![
                delete_op(lhs_id, "lhs", lhs_row.clone()),
                delete_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // A single delete from lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));

            // Clean up tx
            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
        }

        // Case 8: Delete a row from lhs and delete a matching row outside the region of rhs
        {
            let lhs_row = product!(3, 8);
            let rhs_row = product!(13, 3, 5);

            delete_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone())?;

            let updates = vec![
                delete_op(lhs_id, "lhs", lhs_row.clone()),
                delete_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
        }

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
        let q_all = QueryExpr::new(db_table((&schema).into(), "_inventory".to_owned(), schema.table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("_inventory", "inventory_id"), scalar(1u64));

        let s = QuerySet::from([q_all, q_id]);

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

        check_query_incr(&db, &mut tx, &s, &update, 1, &[row])?;

        let q = QueryExpr::new(db_table((&schema).into(), "_inventory".to_owned(), schema.table_id));

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
        let q_all = QueryExpr::new(db_table((&schema).into(), "inventory".to_owned(), schema.table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("inventory", "inventory_id"), scalar(1u64));

        let s = QuerySet::from([q_all, q_id]);

        check_query_eval(&db, &mut tx, &s, 1, &[product!(1u64, "health")])?;

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

        check_query_incr(&db, &mut tx, &s, &update, 1, &[row])?;

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
        let q = compile_read_only_query(&db, &tx, &AuthCtx::for_testing(), sql_query)?;

        for q in &q {
            assert_eq!(
                run_query(&db, &mut tx, q, AuthCtx::for_testing())?.len(),
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

        let s = compile_read_only_query(&db, &tx, &AuthCtx::for_testing(), SUBSCRIBE_TO_ALL_QUERY)
            .map(QuerySet::from_iter)?;

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

    #[test]
    fn test_classify() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [plain]
        let schema = &[("id", AlgebraicType::U64)];
        create_table(&db, &mut tx, "plain", schema, &[])?;

        // Create table [lhs] with indexes on [id] and [x]
        let schema = &[("id", AlgebraicType::U64), ("x", AlgebraicType::I32)];
        let indexes = &[(0, "id"), (1, "x")];
        create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with indexes on [id] and [y]
        let schema = &[("id", AlgebraicType::U64), ("y", AlgebraicType::I32)];
        let indexes = &[(0, "id"), (1, "y")];
        create_table(&db, &mut tx, "rhs", schema, indexes)?;

        let auth = AuthCtx::for_testing();

        // All single table queries are supported
        let scans = [
            "SELECT * FROM plain",
            "SELECT * FROM plain WHERE id > 5",
            "SELECT plain.* FROM plain",
            "SELECT plain.* FROM plain WHERE plain.id = 5",
            "SELECT * FROM lhs",
            "SELECT * FROM lhs WHERE id > 5",
        ];
        for scan in scans {
            let expr = compile_read_only_query(&db, &tx, &auth, scan)?.pop_first().unwrap();
            assert_eq!(classify(&expr), Some(Supported::Scan), "{scan}\n{expr:#?}");
        }

        // Only index semijoins are supported
        let joins = ["SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE rhs.y < 10"];
        for join in joins {
            let expr = compile_read_only_query(&db, &tx, &auth, join)?.pop_first().unwrap();
            assert_eq!(classify(&expr), Some(Supported::Semijoin), "{join}\n{expr:#?}");
        }

        // All other joins are unsupported
        let joins = [
            "SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE lhs.x < 10",
        ];
        for join in joins {
            let expr = compile_read_only_query(&db, &tx, &auth, join)?.pop_first().unwrap();
            assert_eq!(classify(&expr), None, "{join}\n{expr:#?}");
        }

        Ok(())
    }
}
