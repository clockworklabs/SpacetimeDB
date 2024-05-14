use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::sql::compiler::compile_sql;
use crate::subscription::subscription::SupportedQuery;
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_vm::expr::{self, Crud, CrudExpr, DbType, QueryExpr};

pub(crate) static WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
pub const SUBSCRIBE_TO_ALL_QUERY: &str = "SELECT * FROM *";

// TODO: It's semantically wrong to `SUBSCRIBE_TO_ALL_QUERY`
// as it can only return back the changes valid for the tables in scope *right now*
// instead of **continuously updating** the db changes
// with system table modifications (add/remove tables, indexes, ...).
//
/// Variant of [`compile_read_only_query`] which appends `SourceExpr`s into a given `SourceBuilder`,
/// rather than returning a new `SourceSet`.
///
/// This is necessary when merging multiple SQL queries into a single query set,
/// as in [`crate::subscription::module_subscription_actor::ModuleSubscriptions::add_subscriber`].
pub fn compile_read_only_query(
    relational_db: &RelationalDB,
    tx: &Tx,
    input: &str,
) -> Result<Vec<SupportedQuery>, DBError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SubscriptionError::Empty.into());
    }

    // Remove redundant whitespace, and in particular newlines, for debug info.
    let input = WHITESPACE.replace_all(input, " ");

    let compiled = compile_sql(relational_db, tx, &input)?;
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
            CrudExpr::SetVar { .. } => return Err(SubscriptionError::SideEffect(Crud::Config).into()),
            CrudExpr::ReadVar { .. } => return Err(SubscriptionError::SideEffect(Crud::Config).into()),
        }
    }

    if !queries.is_empty() {
        Ok(queries
            .into_iter()
            .map(|query| SupportedQuery::new(query, input.to_string()))
            .collect::<Result<_, _>>()?)
    } else {
        Err(SubscriptionError::Empty.into())
    }
}

/// The kind of [`QueryExpr`] currently supported for incremental evaluation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum Supported {
    /// A scan or [`QueryExpr::Select`] of a single table.
    Select,
    /// A semijoin of two tables, restricted to [`QueryExpr::IndexJoin`]s.
    ///
    /// See [`crate::sql::compiler::try_index_join`].
    Semijoin,
}

/// Classify a [`QueryExpr`] into a [`Supported`] kind, or `None` if incremental
/// evaluation is not currently supported for the expression.
pub fn classify(expr: &QueryExpr) -> Option<Supported> {
    use expr::Query::*;
    if matches!(&*expr.query, [IndexJoin(_)]) {
        return Some(Supported::Semijoin);
    }
    for op in &expr.query {
        if let JoinInner(_) = op {
            return None;
        }
    }
    Some(Supported::Select)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Protocol;
    use crate::db::datastore::traits::IsolationLevel;
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::db::relational_db::MutTx;
    use crate::execution_context::ExecutionContext;
    use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate};
    use crate::sql::execute::collect_result;
    use crate::sql::execute::tests::run_for_testing;
    use crate::subscription::subscription::{get_all, ExecutionSet};
    use crate::util::slow::SlowQueryConfig;
    use crate::vm::tests::create_table_with_rows;
    use crate::vm::DbProgram;
    use itertools::Itertools;
    use spacetimedb_lib::bsatn::to_vec;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::Identity;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::*;
    use spacetimedb_sats::relation::FieldName;
    use spacetimedb_sats::{product, AlgebraicType, ProductType, ProductValue};
    use spacetimedb_vm::eval::run_ast;
    use spacetimedb_vm::eval::test_helpers::{mem_table, mem_table_without_table_name, scalar};
    use spacetimedb_vm::expr::{Expr, SourceSet};
    use spacetimedb_vm::operator::OpCmp;
    use spacetimedb_vm::relation::{MemTable, RelValue};
    use std::sync::Arc;

    /// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
    fn run_query<const N: usize>(
        cx: &ExecutionContext,
        db: &RelationalDB,
        tx: &Tx,
        query: &QueryExpr,
        auth: AuthCtx,
        sources: SourceSet<Vec<ProductValue>, N>,
    ) -> Result<Vec<MemTable>, DBError> {
        let mut tx = tx.into();
        let p = &mut DbProgram::new(cx, db, &mut tx, auth);
        let q = Expr::Crud(Box::new(CrudExpr::Query(query.clone())));

        let mut result = Vec::with_capacity(1);
        collect_result(&mut result, run_ast(p, q, sources).into())?;
        Ok(result)
    }

    fn insert_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
        DatabaseTableUpdate {
            table_id,
            table_name: table_name.into(),
            deletes: [].into(),
            inserts: [row].into(),
        }
    }

    fn delete_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
        DatabaseTableUpdate {
            table_id,
            table_name: table_name.into(),
            deletes: [row].into(),
            inserts: [].into(),
        }
    }

    fn insert_row(db: &RelationalDB, tx: &mut MutTx, table_id: TableId, row: ProductValue) -> ResultTest<()> {
        db.insert(tx, table_id, row)?;
        Ok(())
    }

    fn delete_row(db: &RelationalDB, tx: &mut MutTx, table_id: TableId, row: ProductValue) {
        db.delete_by_rel(tx, table_id, [row]);
    }

    fn make_data(
        db: &RelationalDB,
        tx: &mut MutTx,
        table_name: &str,
        head: &ProductType,
        row: &ProductValue,
    ) -> ResultTest<(Arc<TableSchema>, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let schema = create_table_with_rows(db, tx, table_name, head.clone(), &[row.clone()])?;
        let table = mem_table(schema.table_id, schema.get_row_type().clone(), [row.clone()]);

        let data = DatabaseTableUpdate {
            table_id: schema.table_id,
            table_name: table_name.into(),
            deletes: [].into(),
            inserts: [row.clone()].into(),
        };

        let q = QueryExpr::new(&*schema);

        Ok((schema, table, data, q))
    }

    fn make_inv(
        db: &RelationalDB,
        tx: &mut MutTx,
        access: StAccess,
    ) -> ResultTest<(Arc<TableSchema>, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table_name = if access == StAccess::Public {
            "inventory"
        } else {
            "_inventory"
        };

        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row)?;

        // For filtering out the hidden field `OP_TYPE_FIELD_NAME`
        let fields = &[0, 1].map(|c| FieldName::new(schema.table_id, c.into()).into());

        let q = q.with_project(fields, None);

        Ok((schema, table, data, q))
    }

    fn make_player(
        db: &RelationalDB,
        tx: &mut MutTx,
    ) -> ResultTest<(Arc<TableSchema>, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table_name = "player";
        let head = ProductType::from([("player_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(2u64, "jhon doe");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row)?;

        let fields = &[0, 1].map(|c| FieldName::new(schema.table_id, c.into()).into());

        let q = q.with_project(fields, None);

        Ok((schema, table, data, q))
    }

    /// Replace the primary (ie. `source`) table of the given [`QueryExpr`] with
    /// a virtual [`MemTable`] consisting of the rows in [`DatabaseTableUpdate`].
    fn query_to_mem_table(
        mut of: QueryExpr,
        data: &DatabaseTableUpdate,
    ) -> (QueryExpr, SourceSet<Vec<ProductValue>, 1>) {
        let data = data.deletes.iter().chain(data.inserts.iter()).cloned().collect();
        let mem_table = MemTable::new(of.source.head().clone(), of.source.table_access(), data);
        let mut sources = SourceSet::empty();
        of.source = sources.add_mem_table(mem_table);
        (of, sources)
    }

    fn check_query(
        db: &RelationalDB,
        table: &MemTable,
        tx: &Tx,
        q: &QueryExpr,
        data: &DatabaseTableUpdate,
    ) -> ResultTest<()> {
        let (q, sources) = query_to_mem_table(q.clone(), data);
        let result = run_query(
            &ExecutionContext::default(),
            db,
            tx,
            &q,
            AuthCtx::for_testing(),
            sources,
        )?;

        assert_eq!(
            Some(mem_table_without_table_name(table)),
            result.first().map(mem_table_without_table_name)
        );

        Ok(())
    }

    fn check_query_incr(
        db: &RelationalDB,
        tx: &Tx,
        s: &ExecutionSet,
        update: &DatabaseUpdate,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let ctx = &ExecutionContext::incremental_update(db.address(), SlowQueryConfig::default());
        let tx = &tx.into();
        let update = update.tables.iter().collect::<Vec<_>>();
        let result = s.eval_incr(ctx, db, tx, &update)?;
        assert_eq!(
            result.tables.len(),
            total_tables,
            "Must return the correct number of tables: {result:#?}"
        );

        let result = result
            .tables
            .into_iter()
            .flat_map(|update| <Vec<ProductValue>>::from(&update.updates))
            .sorted()
            .collect::<Vec<_>>();

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    fn check_query_eval(
        ctx: &ExecutionContext,
        db: &RelationalDB,
        tx: &Tx,
        s: &ExecutionSet,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let result = s.eval(ctx, Protocol::Binary, db, tx)?.tables.unwrap_left();
        assert_eq!(
            result.len(),
            total_tables,
            "Must return the correct number of tables: {result:#?}"
        );

        let result = result
            .into_iter()
            .flat_map(|x| x.table_row_operations.into_iter().map(|x| x.row))
            .sorted()
            .collect_vec();

        let rows = rows.iter().map(|r| to_vec(r).unwrap()).collect_vec();

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    fn singleton_execution_set(expr: QueryExpr, sql: String) -> ResultTest<ExecutionSet> {
        Ok(ExecutionSet::from_iter([SupportedQuery::try_from((expr, sql))?]))
    }

    #[test]
    fn test_eval_incr_for_index_scan() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let table_id = db.create_table_for_test("test", schema, indexes)?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let mut deletes = Vec::new();
        for i in 0u64..9u64 {
            db.insert(&mut tx, table_id, product!(i, i))?;
            deletes.push(product!(i + 10, i))
        }

        let update = DatabaseUpdate {
            tables: vec![DatabaseTableUpdate {
                table_id,
                table_name: "test".into(),
                deletes: deletes.into(),
                inserts: [].into(),
            }],
        };

        db.commit_tx(&ExecutionContext::default(), tx)?;
        let tx = db.begin_tx();

        let sql = "select * from test where b = 3";
        let mut exp = compile_sql(&db, &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query: ExecutionSet = singleton_execution_set(query, sql.into())?;

        let ctx = &ExecutionContext::incremental_update(db.address(), SlowQueryConfig::default());
        let tx = (&tx).into();
        let update = update.tables.iter().collect::<Vec<_>>();
        let result = query.eval_incr(ctx, &db, &tx, &update)?;

        assert_eq!(result.tables.len(), 1);

        let update = &result.tables[0].updates;

        assert_eq!(update.inserts.len(), 0);
        assert_eq!(update.deletes.len(), 1);

        let op = &update.deletes[0];

        assert_eq!(op.clone().into_product_value(), product!(13u64, 3u64));
        Ok(())
    }

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Public)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Public);

        let tx = db.begin_tx();
        let q_1 = q.clone();
        check_query(&db, &table, &tx, &q_1, &data)?;

        let q_2 = q.with_select_cmp(OpCmp::Eq, FieldName::new(schema.table_id, 0.into()), scalar(1u64));
        check_query(&db, &table, &tx, &q_2, &data)?;

        Ok(())
    }

    // Check that the `owner` can access private tables (that start with `_`) and that it fails if the `caller` is different
    #[test]
    fn test_subscribe_private() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Private)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Private);

        let row = product!(1u64, "health");
        let tx = db.begin_tx();
        check_query(&db, &table, &tx, &q, &data)?;

        // SELECT * FROM inventory WHERE inventory_id = 1
        let q_id = QueryExpr::new(&*schema).with_select_cmp(
            OpCmp::Eq,
            FieldName::new(schema.table_id, 0.into()),
            scalar(1u64),
        );

        let s = singleton_execution_set(q_id, "SELECT * FROM inventory WHERE inventory_id = 1".into())?;

        let data = DatabaseTableUpdate {
            table_id: schema.table_id,
            table_name: "_inventory".into(),
            deletes: [].into(),
            inserts: [row.clone()].into(),
        };

        let update = DatabaseUpdate {
            tables: vec![data.clone()],
        };

        check_query_incr(&db, &tx, &s, &update, 1, &[row])?;

        let q = QueryExpr::new(&*schema);

        let (q, sources) = query_to_mem_table(q, &data);
        //Try access the private table
        match run_query(
            &ExecutionContext::default(),
            &db,
            &tx,
            &q,
            AuthCtx::new(Identity::__dummy(), Identity::from_byte_array([1u8; 32])),
            sources,
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

    #[test]
    fn test_subscribe_sql() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [MobileEntityState]
        let schema = &[
            ("entity_id", AlgebraicType::U64),
            ("location_x", AlgebraicType::I32),
            ("location_z", AlgebraicType::I32),
            ("destination_x", AlgebraicType::I32),
            ("destination_z", AlgebraicType::I32),
            ("is_running", AlgebraicType::Bool),
            ("timestamp", AlgebraicType::U64),
            ("dimension", AlgebraicType::U32),
        ];
        let indexes = &[
            (0.into(), "entity_id"),
            (1.into(), "location_x"),
            (2.into(), "location_z"),
        ];
        db.create_table_for_test("MobileEntityState", schema, indexes)?;

        // Create table [EnemyState]
        let schema = &[
            ("entity_id", AlgebraicType::U64),
            ("herd_id", AlgebraicType::I32),
            ("status", AlgebraicType::I32),
            ("type", AlgebraicType::I32),
            ("direction", AlgebraicType::I32),
        ];
        let indexes = &[(0.into(), "entity_id")];
        db.create_table_for_test("EnemyState", schema, indexes)?;

        let sql_insert = "\
        insert into MobileEntityState (entity_id, location_x, location_z, destination_x, destination_z, is_running, timestamp, dimension) values (1, 96001, 96001, 96001, 1867045146, false, 17167179743690094247, 3926297397);\
        insert into MobileEntityState (entity_id, location_x, location_z, destination_x, destination_z, is_running, timestamp, dimension) values (2, 96001, 191000, 191000, 1560020888, true, 2947537077064292621, 445019304);

        insert into EnemyState (entity_id, herd_id, status, type, direction) values (1, 1181485940, 1633678837, 1158301365, 132191327);
        insert into EnemyState (entity_id, herd_id, status, type, direction) values (2, 2017368418, 194072456, 34423057, 1296770410);";
        run_for_testing(&db, sql_insert)?;

        let sql_query = "\
            SELECT EnemyState.* FROM EnemyState \
            JOIN MobileEntityState ON MobileEntityState.entity_id = EnemyState.entity_id  \
            WHERE MobileEntityState.location_x > 96000 \
            AND MobileEntityState.location_x < 192000 \
            AND MobileEntityState.location_z > 96000 \
            AND MobileEntityState.location_z < 192000";

        let tx = db.begin_tx();
        let qset = compile_read_only_query(&db, &tx, sql_query)?;

        for q in qset {
            let result = run_query(
                &ExecutionContext::default(),
                &db,
                &tx,
                q.as_expr(),
                AuthCtx::for_testing(),
                SourceSet::<_, 0>::empty(),
            )?;
            assert_eq!(result.len(), 1, "Join query did not return any rows");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_subscribe_all() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        let (schema_1, _, _, _) = make_inv(&db, &mut tx, StAccess::Public)?;
        let (schema_2, _, _, _) = make_player(&db, &mut tx)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");
        let tx = db.begin_tx();
        let s = get_all(&db, &tx, &AuthCtx::for_testing())?.into();
        let ctx = ExecutionContext::subscribe(db.address(), SlowQueryConfig::default());
        check_query_eval(&ctx, &db, &tx, &s, 2, &[row_1.clone(), row_2.clone()])?;

        let data1 = DatabaseTableUpdate {
            table_id: schema_1.table_id,
            table_name: "inventory".into(),
            deletes: [row_1].into(),
            inserts: [].into(),
        };

        let data2 = DatabaseTableUpdate {
            table_id: schema_2.table_id,
            table_name: "player".into(),
            deletes: [].into(),
            inserts: [row_2].into(),
        };

        let update = DatabaseUpdate {
            tables: vec![data1, data2],
        };

        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");
        check_query_incr(&db, &tx, &s, &update, 2, &[row_1, row_2])?;

        Ok(())
    }

    #[test]
    fn test_classify() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [plain]
        let schema = &[("id", AlgebraicType::U64)];
        db.create_table_for_test("plain", schema, &[])?;

        // Create table [lhs] with indexes on [id] and [x]
        let schema = &[("id", AlgebraicType::U64), ("x", AlgebraicType::I32)];
        let indexes = &[(ColId(0), "id"), (ColId(1), "x")];
        db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with indexes on [id] and [y]
        let schema = &[("id", AlgebraicType::U64), ("y", AlgebraicType::I32)];
        let indexes = &[(ColId(0), "id"), (ColId(1), "y")];
        db.create_table_for_test("rhs", schema, indexes)?;

        let tx = db.begin_tx();

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
            let expr = compile_read_only_query(&db, &tx, scan)?.pop().unwrap();
            assert_eq!(expr.kind(), Supported::Select, "{scan}\n{expr:#?}");
        }

        // Only index semijoins are supported
        let joins = ["SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE rhs.y < 10"];
        for join in joins {
            let expr = compile_read_only_query(&db, &tx, join)?.pop().unwrap();
            assert_eq!(expr.kind(), Supported::Semijoin, "{join}\n{expr:#?}");
        }

        // All other joins are unsupported
        let joins = [
            "SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE lhs.x < 10",
        ];
        for join in joins {
            match compile_read_only_query(&db, &tx, join) {
                Err(DBError::Subscription(SubscriptionError::Unsupported(_))) => (),
                x => panic!("Unexpected: {x:?}"),
            }
        }

        Ok(())
    }

    /// Create table [lhs] with index on [id]
    fn create_lhs_table_for_eval_incr(db: &RelationalDB) -> ResultTest<TableId> {
        const I32: AlgebraicType = AlgebraicType::I32;
        let lhs_id = db.create_table_for_test("lhs", &[("id", I32), ("x", I32)], &[(0.into(), "id")])?;
        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            for i in 0..5 {
                db.insert(tx, lhs_id, product!(i, i + 5))?;
            }
            Ok(lhs_id)
        })
    }

    /// Create table [rhs] with index on [id]
    fn create_rhs_table_for_eval_incr(db: &RelationalDB) -> ResultTest<TableId> {
        const I32: AlgebraicType = AlgebraicType::I32;
        let rhs_id = db.create_table_for_test("rhs", &[("rid", I32), ("id", I32), ("y", I32)], &[(1.into(), "id")])?;
        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            for i in 10..20 {
                db.insert(tx, rhs_id, product!(i, i - 10, i - 8))?;
            }
            Ok(rhs_id)
        })
    }

    fn compile_query(db: &RelationalDB) -> ResultTest<ExecutionSet> {
        db.with_read_only(&ExecutionContext::default(), |tx| {
            // Should be answered using an index semijion
            let sql = "select lhs.* from lhs join rhs on lhs.id = rhs.id where rhs.y >= 2 and rhs.y <= 4";
            let mut exp = compile_sql(db, tx, sql)?;
            let Some(CrudExpr::Query(query)) = exp.pop() else {
                panic!("unexpected query {:#?}", exp[0]);
            };
            singleton_execution_set(query, sql.into())
        })
    }

    fn run_eval_incr_test<T, F: Fn(&RelationalDB) -> ResultTest<T>>(test_fn: F) -> ResultTest<T> {
        TestDB::durable().map(|db| test_fn(&db))??;
        TestDB::durable().map(|db| test_fn(&db.with_row_count(Arc::new(|_, _| 5))))?
    }

    #[test]
    fn test_eval_incr_for_index_join() -> ResultTest<()> {
        // Case 1:
        // Delete a row inside the region of rhs,
        // Insert a row inside the region of rhs.
        run_eval_incr_test(index_join_case_1)?;
        // Case 2:
        // Delete a row outside the region of rhs,
        // Insert a row outside the region of rhs.
        run_eval_incr_test(index_join_case_2)?;
        // Case 3:
        // Delete a row inside  the region of rhs,
        // Insert a row outside the region of rhs.
        run_eval_incr_test(index_join_case_3)?;
        // Case 4:
        // Delete a row outside the region of rhs,
        // Insert a row inside  the region of rhs.
        run_eval_incr_test(index_join_case_4)?;
        // Case 5:
        // Insert row into lhs,
        // Insert matching row inside the region of rhs.
        run_eval_incr_test(index_join_case_5)?;
        // Case 6:
        // Insert row into lhs,
        // Insert matching row outside the region of rhs.
        run_eval_incr_test(index_join_case_6)?;
        // Case 7:
        // Delete row from lhs,
        // Delete matching row inside the region of rhs.
        run_eval_incr_test(index_join_case_7)?;
        // Case 8:
        // Delete row from lhs,
        // Delete matching row outside the region of rhs.
        run_eval_incr_test(index_join_case_8)?;
        // Case 9:
        // Update row from lhs,
        // Update matching row inside the region of rhs.
        run_eval_incr_test(index_join_case_9)?;
        Ok(())
    }

    fn eval_incr(
        db: &RelationalDB,
        query: &ExecutionSet,
        tables: Vec<DatabaseTableUpdate>,
    ) -> ResultTest<DatabaseUpdate> {
        let ctx = &ExecutionContext::default();
        let update = DatabaseUpdate { tables };
        db.with_read_only(ctx, |tx| {
            let tx = (&*tx).into();
            let update = update.tables.iter().collect::<Vec<_>>();
            let result = query.eval_incr(ctx, db, &tx, &update)?;
            let tables = result
                .tables
                .into_iter()
                .map(|update| {
                    let convert = |rvs: Vec<_>| rvs.into_iter().map(RelValue::into_product_value).collect();
                    DatabaseTableUpdate {
                        table_id: update.table_id,
                        table_name: update.table_name,
                        deletes: convert(update.updates.deletes),
                        inserts: convert(update.updates.inserts),
                    }
                })
                .collect();
            Ok(DatabaseUpdate { tables })
        })
    }

    // Case 1:
    // Delete a row inside the region of rhs,
    // Insert a row inside the region of rhs.
    fn index_join_case_1(db: &RelationalDB) -> ResultTest<()> {
        let _ = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let r1 = product!(10, 0, 2);
        let r2 = product!(10, 0, 3);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(db, tx, rhs_id, r1.clone());
            insert_row(db, tx, rhs_id, r2.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ],
        )?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 2:
    // Delete a row outside the region of rhs,
    // Insert a row outside the region of rhs.
    fn index_join_case_2(db: &RelationalDB) -> ResultTest<()> {
        let _ = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let r1 = product!(13, 3, 5);
        let r2 = product!(13, 3, 6);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(db, tx, rhs_id, r1.clone());
            insert_row(db, tx, rhs_id, r2.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ],
        )?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 3:
    // Delete a row inside  the region of rhs,
    // Insert a row outside the region of rhs.
    fn index_join_case_3(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let r1 = product!(10, 0, 2);
        let r2 = product!(10, 0, 5);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(db, tx, rhs_id, r1.clone());
            insert_row(db, tx, rhs_id, r2.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ],
        )?;

        // A single delete from lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));
        Ok(())
    }

    // Case 4:
    // Delete a row outside the region of rhs,
    // Insert a row inside  the region of rhs.
    fn index_join_case_4(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let r1 = product!(13, 3, 5);
        let r2 = product!(13, 3, 4);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(db, tx, rhs_id, r1.clone());
            insert_row(db, tx, rhs_id, r2.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ],
        )?;

        // A single insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(3, 8)));
        Ok(())
    }

    // Case 5:
    // Insert row into lhs,
    // Insert matching row inside the region of rhs.
    fn index_join_case_5(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let lhs_row = product!(5, 10);
        let rhs_row = product!(20, 5, 3);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            insert_row(db, tx, lhs_id, lhs_row.clone())?;
            insert_row(db, tx, rhs_id, rhs_row.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                insert_op(lhs_id, "lhs", lhs_row.clone()),
                insert_op(rhs_id, "rhs", rhs_row.clone()),
            ],
        )?;

        // A single insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(5, 10)));
        Ok(())
    }

    // Case 6:
    // Insert row into lhs,
    // Insert matching row outside the region of rhs.
    fn index_join_case_6(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let lhs_row = product!(5, 10);
        let rhs_row = product!(20, 5, 5);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            insert_row(db, tx, lhs_id, lhs_row.clone())?;
            insert_row(db, tx, rhs_id, rhs_row.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                insert_op(lhs_id, "lhs", lhs_row.clone()),
                insert_op(rhs_id, "rhs", rhs_row.clone()),
            ],
        )?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 7:
    // Delete row from lhs,
    // Delete matching row inside the region of rhs.
    fn index_join_case_7(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let lhs_row = product!(0, 5);
        let rhs_row = product!(10, 0, 2);

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> ResultTest<_> {
            delete_row(db, tx, lhs_id, lhs_row.clone());
            delete_row(db, tx, rhs_id, rhs_row.clone());
            Ok(())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                delete_op(lhs_id, "lhs", lhs_row.clone()),
                delete_op(rhs_id, "rhs", rhs_row.clone()),
            ],
        )?;

        // A single delete from lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));
        Ok(())
    }

    // Case 8:
    // Delete row from lhs,
    // Delete matching row outside the region of rhs.
    fn index_join_case_8(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let lhs_row = product!(3, 8);
        let rhs_row = product!(13, 3, 5);

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> ResultTest<_> {
            delete_row(db, tx, lhs_id, lhs_row.clone());
            delete_row(db, tx, rhs_id, rhs_row.clone());
            Ok(())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                delete_op(lhs_id, "lhs", lhs_row.clone()),
                delete_op(rhs_id, "rhs", rhs_row.clone()),
            ],
        )?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 9:
    // Update row from lhs,
    // Update matching row inside the region of rhs.
    fn index_join_case_9(db: &RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(db)?;
        let rhs_id = create_rhs_table_for_eval_incr(db)?;
        let query = compile_query(db)?;

        let lhs_old = product!(1, 6);
        let lhs_new = product!(1, 7);
        let rhs_old = product!(11, 1, 3);
        let rhs_new = product!(11, 1, 4);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(db, tx, lhs_id, lhs_old.clone());
            delete_row(db, tx, rhs_id, rhs_old.clone());
            insert_row(db, tx, lhs_id, lhs_new.clone())?;
            insert_row(db, tx, rhs_id, rhs_new.clone())
        })?;

        let result = eval_incr(
            db,
            &query,
            vec![
                DatabaseTableUpdate {
                    table_id: lhs_id,
                    table_name: "lhs".into(),
                    deletes: [lhs_old.clone()].into(),
                    inserts: [lhs_new.clone()].into(),
                },
                DatabaseTableUpdate {
                    table_id: rhs_id,
                    table_name: "rhs".into(),
                    deletes: [rhs_old.clone()].into(),
                    inserts: [rhs_new.clone()].into(),
                },
            ],
        )?;

        // A delete and an insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(
            result.tables[0],
            DatabaseTableUpdate {
                table_id: lhs_id,
                table_name: "lhs".into(),
                deletes: [lhs_old].into(),
                inserts: [lhs_new].into(),
            },
        );
        Ok(())
    }
}
