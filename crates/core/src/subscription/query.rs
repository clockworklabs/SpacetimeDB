use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::sql::ast::SchemaViewer;
use crate::sql::compiler::compile_sql;
use crate::subscription::subscription::SupportedQuery;
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_subscription::SubscriptionPlan;
use spacetimedb_vm::expr::{self, Crud, CrudExpr, QueryExpr};

use super::execution_unit::QueryHash;
use super::module_subscription_manager::Plan;

static WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*$").unwrap());
static SUBSCRIBE_TO_ALL_TABLES_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?i)\bSELECT\s+\*\s+FROM\s+\*\s*$").unwrap());

/// Is this string all whitespace?
pub fn is_whitespace_or_empty(sql: &str) -> bool {
    WHITESPACE.is_match_at(sql, 0)
}

/// Is this a `SELECT * FROM *` query?
pub fn is_subscribe_to_all_tables(sql: &str) -> bool {
    SUBSCRIBE_TO_ALL_TABLES_REGEX.is_match_at(sql, 0)
}

// TODO: Remove this after the SubscribeSingle migration.
// TODO: It's semantically wrong to `SELECT * FROM *`
// as it can only return back the changes valid for the tables in scope *right now*
// instead of **continuously updating** the db changes
// with system table modifications (add/remove tables, indexes, ...).
//
/// Variant of [`compile_read_only_query`] which appends `SourceExpr`s into a given `SourceBuilder`,
/// rather than returning a new `SourceSet`.
///
/// This is necessary when merging multiple SQL queries into a single query set,
/// as in [`crate::subscription::module_subscription_actor::ModuleSubscriptions::add_subscriber`].
pub fn compile_read_only_queryset(
    relational_db: &RelationalDB,
    auth: &AuthCtx,
    tx: &Tx,
    input: &str,
) -> Result<Vec<SupportedQuery>, DBError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SubscriptionError::Empty.into());
    }

    // Remove redundant whitespace, and in particular newlines, for debug info.
    let input = WHITESPACE.replace_all(input, " ");

    let compiled = compile_sql(relational_db, auth, tx, &input)?;
    let mut queries = Vec::with_capacity(compiled.len());
    for q in compiled {
        return Err(SubscriptionError::SideEffect(match q {
            CrudExpr::Query(x) => {
                queries.push(x);
                continue;
            }
            CrudExpr::Insert { .. } => Crud::Insert,
            CrudExpr::Update { .. } => Crud::Update,
            CrudExpr::Delete { .. } => Crud::Delete,
            CrudExpr::SetVar { .. } => Crud::Config,
            CrudExpr::ReadVar { .. } => Crud::Config,
        })
        .into());
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

/// Compile a string into a single read-only query.
/// This returns an error if the string has multiple queries or mutations.
pub fn compile_read_only_query(auth: &AuthCtx, tx: &Tx, input: &str) -> Result<Plan, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let tx = SchemaViewer::new(tx, auth);
    let (plans, has_param) = SubscriptionPlan::compile(input, &tx, auth)?;
    let hash = QueryHash::from_string(input, auth.caller, has_param);
    Ok(Plan::new(plans, hash, input.to_owned()))
}

/// Compile a string into a single read-only query.
/// This returns an error if the string has multiple queries or mutations.
pub fn compile_query_with_hashes(
    auth: &AuthCtx,
    tx: &Tx,
    input: &str,
    hash: QueryHash,
    hash_with_param: QueryHash,
) -> Result<Plan, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let tx = SchemaViewer::new(tx, auth);
    let (plans, has_param) = SubscriptionPlan::compile(input, &tx, auth)?;

    if has_param {
        return Ok(Plan::new(plans, hash_with_param, input.to_owned()));
    }
    Ok(Plan::new(plans, hash, input.to_owned()))
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
    use crate::db::relational_db::tests_utils::{
        begin_mut_tx, begin_tx, insert, with_auto_commit, with_read_only, TestDB,
    };
    use crate::db::relational_db::MutTx;
    use crate::execution_context::Workload;
    use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, UpdatesRelValue};
    use crate::sql::execute::collect_result;
    use crate::sql::execute::tests::run_for_testing;
    use crate::subscription::module_subscription_manager::QueriedTableIndexIds;
    use crate::subscription::subscription::{legacy_get_all, ExecutionSet};
    use crate::subscription::tx::DeltaTx;
    use crate::vm::tests::create_table_with_rows;
    use crate::vm::DbProgram;
    use itertools::Itertools;
    use spacetimedb_client_api_messages::websocket::{BsatnFormat, Compression};
    use spacetimedb_lib::bsatn;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::metrics::ExecutionMetrics;
    use spacetimedb_lib::relation::FieldName;
    use spacetimedb_lib::Identity;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::{product, AlgebraicType, ProductType, ProductValue};
    use spacetimedb_schema::schema::*;
    use spacetimedb_vm::eval::run_ast;
    use spacetimedb_vm::eval::test_helpers::{mem_table, mem_table_without_table_name, scalar};
    use spacetimedb_vm::expr::{Expr, SourceSet};
    use spacetimedb_vm::operator::OpCmp;
    use spacetimedb_vm::relation::{MemTable, RelValue};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
    fn run_query<const N: usize>(
        db: &RelationalDB,
        tx: &Tx,
        query: &QueryExpr,
        auth: AuthCtx,
        sources: SourceSet<Vec<ProductValue>, N>,
    ) -> Result<Vec<MemTable>, DBError> {
        let mut tx = tx.into();
        let p = &mut DbProgram::new(db, &mut tx, auth);
        let q = Expr::Crud(Box::new(CrudExpr::Query(query.clone())));

        let mut result = Vec::with_capacity(1);
        let mut updates = Vec::new();
        collect_result(&mut result, &mut updates, run_ast(p, q, sources).into())?;
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
        insert(db, tx, table_id, &row)?;
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
        access: StAccess,
    ) -> ResultTest<(Arc<TableSchema>, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let schema = create_table_with_rows(db, tx, table_name, head.clone(), &[row.clone()], access)?;
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
        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");

        let (schema, table, data, q) = make_data(db, tx, "inventory", &head, &row, access)?;

        let fields = &[0, 1].map(|c| FieldName::new(schema.table_id, c.into()).into());
        let q = q.with_project(fields.into(), None).unwrap();

        Ok((schema, table, data, q))
    }

    fn make_player(
        db: &RelationalDB,
        tx: &mut MutTx,
    ) -> ResultTest<(Arc<TableSchema>, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table_name = "player";
        let head = ProductType::from([("player_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(2u64, "jhon doe");

        let (schema, table, data, q) = make_data(db, tx, table_name, &head, &row, StAccess::Public)?;

        let fields = [0, 1].map(|c| FieldName::new(schema.table_id, c.into()).into());
        let q = q.with_project(fields.into(), None).unwrap();

        Ok((schema, table, data, q))
    }

    /// Replace the primary (ie. `source`) table of the given [`QueryExpr`] with
    /// a virtual [`MemTable`] consisting of the rows in [`DatabaseTableUpdate`].
    fn query_to_mem_table(
        mut of: QueryExpr,
        data: &DatabaseTableUpdate,
    ) -> (QueryExpr, SourceSet<Vec<ProductValue>, 1>) {
        let data = data.deletes.iter().chain(data.inserts.iter()).cloned().collect();
        let mem_table = MemTable::new(of.head().clone(), of.source.table_access(), data);
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
        let result = run_query(db, tx, &q, AuthCtx::for_testing(), sources)?;

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
        let tx = &tx.into();
        let update = update.tables.iter().collect::<Vec<_>>();
        let result = s.eval_incr_for_test(db, tx, &update, None);
        assert_eq!(
            result.tables.len(),
            total_tables,
            "Must return the correct number of tables: {result:#?}"
        );

        let result = result
            .tables
            .iter()
            .map(|u| &u.updates)
            .flat_map(|u| {
                u.deletes
                    .iter()
                    .chain(&*u.inserts)
                    .map(|rv| rv.clone().into_product_value())
                    .collect::<Vec<_>>()
            })
            .sorted()
            .collect::<Vec<_>>();

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    fn check_query_eval(
        db: &RelationalDB,
        tx: &Tx,
        s: &ExecutionSet,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let result = s.eval::<BsatnFormat>(db, tx, None, Compression::Brotli).tables;
        assert_eq!(
            result.len(),
            total_tables,
            "Must return the correct number of tables: {result:#?}"
        );

        let result = result
            .into_iter()
            .flat_map(|x| x.updates)
            .map(|x| x.maybe_decompress())
            .flat_map(|x| {
                (&x.deletes)
                    .into_iter()
                    .chain(&x.inserts)
                    .map(|x| x.to_owned())
                    .collect::<Vec<_>>()
            })
            .sorted()
            .collect_vec();

        let rows = rows.iter().map(|r| bsatn::to_vec(r).unwrap()).collect_vec();

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    fn singleton_execution_set(expr: QueryExpr, sql: String) -> ResultTest<ExecutionSet> {
        Ok(ExecutionSet::from_iter([SupportedQuery::try_from((expr, sql))?]))
    }

    #[test]
    fn test_whitespace_regex() -> ResultTest<()> {
        assert!(is_whitespace_or_empty(""));
        assert!(is_whitespace_or_empty(" "));
        assert!(is_whitespace_or_empty("\n \t"));
        assert!(!is_whitespace_or_empty(" a"));
        Ok(())
    }

    #[test]
    fn test_subscribe_to_all_tables_regex() -> ResultTest<()> {
        assert!(is_subscribe_to_all_tables("SELECT * FROM *"));
        assert!(is_subscribe_to_all_tables("Select * From *"));
        assert!(is_subscribe_to_all_tables("select * from *"));
        assert!(is_subscribe_to_all_tables("\nselect *\nfrom * "));
        assert!(!is_subscribe_to_all_tables("select * from * where"));
        Ok(())
    }

    #[test]
    fn test_compile_incr_plan() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let schema = &[("n", AlgebraicType::U64), ("data", AlgebraicType::U64)];
        let indexes = &[0.into()];
        db.create_table_for_test("a", schema, indexes)?;
        db.create_table_for_test("b", schema, indexes)?;

        let tx = begin_tx(&db);
        let sql = "SELECT b.* FROM b JOIN a ON b.n = a.n WHERE b.data > 200";
        let result = compile_read_only_query(&AuthCtx::for_testing(), &tx, sql);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_eval_incr_for_index_scan() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let table_id = db.create_table_for_test("test", schema, indexes)?;

        let mut tx = begin_mut_tx(&db);
        let mut deletes = Vec::new();
        for i in 0u64..9u64 {
            insert(&db, &mut tx, table_id, &product!(i, i))?;
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

        db.commit_tx(tx)?;
        let tx = begin_tx(&db);

        let sql = "select * from test where b = 3";
        let mut exp = compile_sql(&db, &AuthCtx::for_testing(), &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query: ExecutionSet = singleton_execution_set(query, sql.into())?;

        let tx = (&tx).into();
        let update = update.tables.iter().collect::<Vec<_>>();
        let result = query.eval_incr_for_test(&db, &tx, &update, None);

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

        let mut tx = begin_mut_tx(&db);

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Public)?;
        db.commit_tx(tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Public);

        let tx = begin_tx(&db);
        let q_1 = q.clone();
        check_query(&db, &table, &tx, &q_1, &data)?;

        let q_2 = q
            .with_select_cmp(OpCmp::Eq, FieldName::new(schema.table_id, 0.into()), scalar(1u64))
            .unwrap();
        check_query(&db, &table, &tx, &q_2, &data)?;

        Ok(())
    }

    #[test]
    fn test_subscribe_private() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let mut tx = begin_mut_tx(&db);

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Private)?;
        db.commit_tx(tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Private);

        let row = product!(1u64, "health");
        let tx = begin_tx(&db);
        check_query(&db, &table, &tx, &q, &data)?;

        // SELECT * FROM inventory WHERE inventory_id = 1
        let q_id = QueryExpr::new(&*schema)
            .with_select_cmp(OpCmp::Eq, FieldName::new(schema.table_id, 0.into()), scalar(1u64))
            .unwrap();

        let s = singleton_execution_set(q_id, "SELECT * FROM inventory WHERE inventory_id = 1".into())?;

        let data = DatabaseTableUpdate {
            table_id: schema.table_id,
            table_name: "inventory".into(),
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
        let indexes = &[0.into(), 1.into(), 2.into()];
        db.create_table_for_test("MobileEntityState", schema, indexes)?;

        // Create table [EnemyState]
        let schema = &[
            ("entity_id", AlgebraicType::U64),
            ("herd_id", AlgebraicType::I32),
            ("status", AlgebraicType::I32),
            ("type", AlgebraicType::I32),
            ("direction", AlgebraicType::I32),
        ];
        let indexes = &[0.into()];
        db.create_table_for_test("EnemyState", schema, indexes)?;

        for sql_insert in [
        "insert into MobileEntityState (entity_id, location_x, location_z, destination_x, destination_z, is_running, timestamp, dimension) values (1, 96001, 96001, 96001, 1867045146, false, 17167179743690094247, 3926297397)",
        "insert into MobileEntityState (entity_id, location_x, location_z, destination_x, destination_z, is_running, timestamp, dimension) values (2, 96001, 191000, 191000, 1560020888, true, 2947537077064292621, 445019304)",
        "insert into EnemyState (entity_id, herd_id, status, type, direction) values (1, 1181485940, 1633678837, 1158301365, 132191327)",
        "insert into EnemyState (entity_id, herd_id, status, type, direction) values (2, 2017368418, 194072456, 34423057, 1296770410)"] {
            run_for_testing(&db, sql_insert)?;
        }

        let sql_query = "\
        SELECT EnemyState.* FROM EnemyState \
        JOIN MobileEntityState ON MobileEntityState.entity_id = EnemyState.entity_id  \
        WHERE MobileEntityState.location_x > 96000 \
        AND MobileEntityState.location_x < 192000 \
        AND MobileEntityState.location_z > 96000 \
        AND MobileEntityState.location_z < 192000";

        let tx = begin_tx(&db);
        let qset = compile_read_only_queryset(&db, &AuthCtx::for_testing(), &tx, sql_query)?;

        for q in qset {
            let result = run_query(
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

    #[test]
    fn test_subscribe_all() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let mut tx = begin_mut_tx(&db);

        let (schema_1, _, _, _) = make_inv(&db, &mut tx, StAccess::Public)?;
        let (schema_2, _, _, _) = make_player(&db, &mut tx)?;
        db.commit_tx(tx)?;
        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");
        let tx = db.begin_tx(Workload::Subscribe);
        let s = legacy_get_all(&db, &tx, &AuthCtx::for_testing())?.into();
        check_query_eval(&db, &tx, &s, 2, &[row_1.clone(), row_2.clone()])?;

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
        let indexes = &[ColId(0), ColId(1)];
        db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with indexes on [id] and [y]
        let schema = &[("id", AlgebraicType::U64), ("y", AlgebraicType::I32)];
        let indexes = &[ColId(0), ColId(1)];
        db.create_table_for_test("rhs", schema, indexes)?;

        let tx = begin_tx(&db);

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
            let expr = compile_read_only_queryset(&db, &AuthCtx::for_testing(), &tx, scan)?
                .pop()
                .unwrap();
            assert_eq!(expr.kind(), Supported::Select, "{scan}\n{expr:#?}");
        }

        // Only index semijoins are supported
        let joins = ["SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE rhs.y < 10"];
        for join in joins {
            let expr = compile_read_only_queryset(&db, &AuthCtx::for_testing(), &tx, join)?
                .pop()
                .unwrap();
            assert_eq!(expr.kind(), Supported::Semijoin, "{join}\n{expr:#?}");
        }

        // All other joins are unsupported
        let joins = [
            "SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE lhs.x < 10",
        ];
        for join in joins {
            match compile_read_only_queryset(&db, &AuthCtx::for_testing(), &tx, join) {
                Err(DBError::Subscription(SubscriptionError::Unsupported(_)) | DBError::TypeError(_)) => (),
                x => panic!("Unexpected: {x:?}"),
            }
        }

        Ok(())
    }

    /// Create table [lhs] with index on [id]
    fn create_lhs_table_for_eval_incr(db: &RelationalDB) -> ResultTest<TableId> {
        const I32: AlgebraicType = AlgebraicType::I32;
        let lhs_id = db.create_table_for_test("lhs", &[("id", I32), ("x", I32)], &[0.into()])?;
        with_auto_commit(db, |tx| {
            for i in 0..5 {
                let row = product!(i, i + 5);
                insert(db, tx, lhs_id, &row)?;
            }
            Ok(lhs_id)
        })
    }

    /// Create table [rhs] with index on [id]
    fn create_rhs_table_for_eval_incr(db: &RelationalDB) -> ResultTest<TableId> {
        const I32: AlgebraicType = AlgebraicType::I32;
        let rhs_id = db.create_table_for_test("rhs", &[("rid", I32), ("id", I32), ("y", I32)], &[1.into()])?;
        with_auto_commit(db, |tx| {
            for i in 10..20 {
                let row = product!(i, i - 10, i - 8);
                insert(db, tx, rhs_id, &row)?;
            }
            Ok(rhs_id)
        })
    }

    fn compile_query(db: &RelationalDB) -> ResultTest<SubscriptionPlan> {
        with_read_only(db, |tx| {
            let auth = AuthCtx::for_testing();
            let tx = SchemaViewer::new(tx, &auth);
            // Should be answered using an index semijion
            let sql = "select lhs.* from lhs join rhs on lhs.id = rhs.id where rhs.y >= 2 and rhs.y <= 4";
            Ok(SubscriptionPlan::compile(sql, &tx, &auth)
                .map(|(mut plans, _)| {
                    assert_eq!(plans.len(), 1);
                    plans.pop().unwrap()
                })
                .unwrap())
        })
    }

    fn run_eval_incr_test<T, F: Fn(&RelationalDB) -> ResultTest<T>>(test_fn: F) -> ResultTest<T> {
        TestDB::durable().map(|db| test_fn(&db))??;
        TestDB::durable().map(|db| test_fn(&db.with_row_count(Arc::new(|_, _| 5))))?
    }

    #[test]
    /// TODO: This test is a slight modifaction of [test_eval_incr_for_index_join].
    /// Essentially the WHERE condition is on different tables.
    /// Should refactor to reduce duplicate logic between the two tests.
    fn test_eval_incr_for_left_semijoin() -> ResultTest<()> {
        fn compile_query(db: &RelationalDB) -> ResultTest<SubscriptionPlan> {
            with_read_only(db, |tx| {
                let auth = AuthCtx::for_testing();
                let tx = SchemaViewer::new(tx, &auth);
                // Should be answered using an index semijion
                let sql = "select lhs.* from lhs join rhs on lhs.id = rhs.id where lhs.x >= 5 and lhs.x <= 7";
                Ok(SubscriptionPlan::compile(sql, &tx, &auth)
                    .map(|(mut plans, _)| {
                        assert_eq!(plans.len(), 1);
                        plans.pop().unwrap()
                    })
                    .unwrap())
            })
        }

        // Case 1:
        // Delete a row inside the region of lhs,
        // Insert a row inside the region of lhs.
        fn index_join_case_1(db: &RelationalDB) -> ResultTest<()> {
            let _ = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let r1 = product!(10, 0, 2);
            let r2 = product!(10, 0, 3);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

            // No updates to report
            assert!(result.is_empty());
            Ok(())
        }

        // Case 2:
        // Delete a row outside the region of lhs,
        // Insert a row outside the region of lhs.
        fn index_join_case_2(db: &RelationalDB) -> ResultTest<()> {
            let _ = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let r1 = product!(13, 3, 5);
            let r2 = product!(13, 4, 6);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

            // No updates to report
            assert!(result.is_empty());
            Ok(())
        }

        // Case 3:
        // Delete a row inside  the region of lhs,
        // Insert a row outside the region of lhs.
        fn index_join_case_3(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let r1 = product!(10, 0, 2);
            let r2 = product!(10, 3, 5);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

            // A single delete from lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));
            Ok(())
        }

        // Case 4:
        // Delete a row outside the region of lhs,
        // Insert a row inside  the region of lhs.
        fn index_join_case_4(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let r1 = product!(13, 3, 5);
            let r2 = product!(13, 2, 4);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

            // A single insert into lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(2, 7)));
            Ok(())
        }

        // Case 5:
        // Insert row into rhs,
        // Insert matching row inside the region of lhs.
        fn index_join_case_5(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let lhs_row = product!(5, 6);
            let rhs_row = product!(20, 5, 3);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(
                db,
                &mut metrics,
                &query,
                vec![(lhs_id, lhs_row, true), (rhs_id, rhs_row, true)],
            )?;

            // A single insert into lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(5, 6)));
            Ok(())
        }

        // Case 6:
        // Insert row into rhs,
        // Insert matching row outside the region of lhs.
        fn index_join_case_6(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let lhs_row = product!(5, 10);
            let rhs_row = product!(20, 5, 5);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(
                db,
                &mut metrics,
                &query,
                vec![(lhs_id, lhs_row, true), (rhs_id, rhs_row, true)],
            )?;

            // No updates to report
            assert_eq!(result.tables.len(), 0);
            Ok(())
        }

        // Case 7:
        // Delete row from rhs,
        // Delete matching row inside the region of lhs.
        fn index_join_case_7(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let lhs_row = product!(0, 5);
            let rhs_row = product!(10, 0, 2);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(
                db,
                &mut metrics,
                &query,
                vec![(lhs_id, lhs_row, false), (rhs_id, rhs_row, false)],
            )?;

            // A single delete from lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));
            Ok(())
        }

        // Case 8:
        // Delete row from rhs,
        // Delete matching row outside the region of lhs.
        fn index_join_case_8(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let lhs_row = product!(3, 8);
            let rhs_row = product!(13, 3, 5);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(
                db,
                &mut metrics,
                &query,
                vec![(lhs_id, lhs_row, false), (rhs_id, rhs_row, false)],
            )?;

            // No updates to report
            assert_eq!(result.tables.len(), 0);
            Ok(())
        }

        // Case 9:
        // Update row from rhs,
        // Update matching row inside the region of lhs.
        fn index_join_case_9(db: &RelationalDB) -> ResultTest<()> {
            let lhs_id = create_lhs_table_for_eval_incr(db)?;
            let rhs_id = create_rhs_table_for_eval_incr(db)?;
            let query = compile_query(db)?;

            let lhs_old = product!(1, 6);
            let lhs_new = product!(1, 7);
            let rhs_old = product!(11, 1, 3);
            let rhs_new = product!(11, 1, 4);

            let mut metrics = ExecutionMetrics::default();

            let result = eval_incr(
                db,
                &mut metrics,
                &query,
                vec![
                    (lhs_id, lhs_old, false),
                    (rhs_id, rhs_old, false),
                    (lhs_id, lhs_new, true),
                    (rhs_id, rhs_new, true),
                ],
            )?;

            let lhs_old = product!(1, 6);
            let lhs_new = product!(1, 7);

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

        run_eval_incr_test(index_join_case_1)?;
        run_eval_incr_test(index_join_case_2)?;
        run_eval_incr_test(index_join_case_3)?;
        run_eval_incr_test(index_join_case_4)?;
        run_eval_incr_test(index_join_case_5)?;
        run_eval_incr_test(index_join_case_6)?;
        run_eval_incr_test(index_join_case_7)?;
        run_eval_incr_test(index_join_case_8)?;
        run_eval_incr_test(index_join_case_9)?;
        Ok(())
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
        metrics: &mut ExecutionMetrics,
        plan: &SubscriptionPlan,
        ops: Vec<(TableId, ProductValue, bool)>,
    ) -> ResultTest<DatabaseUpdate> {
        let mut tx = begin_mut_tx(db);

        for (table_id, row, insert) in ops {
            if insert {
                insert_row(db, &mut tx, table_id, row)?;
            } else {
                delete_row(db, &mut tx, table_id, row);
            }
        }

        let (data, _, tx) = tx.commit_downgrade(Workload::ForTests);
        let table_id = plan.subscribed_table_id();
        // This awful construction to convert `Arc<str>` into `Box<str>`.
        let table_name = (&**plan.subscribed_table_name()).into();
        let tx = DeltaTx::new(&tx, &data, &QueriedTableIndexIds::from_iter(plan.index_ids()));

        // IMPORTANT: FOR TESTING ONLY!
        //
        // This utility implements set semantics for incremental updates.
        // This is safe because we are only testing PK/FK joins,
        // and we don't have to track row multiplicities for PK/FK joins.
        // But in general we must assume bag semantics for server side tests.
        let mut eval_delta = || {
            // Note, we can't determine apriori what capacity to allocate
            let mut inserts = HashMap::new();
            let mut deletes = vec![];

            plan.for_each_insert(&tx, metrics, &mut |row| {
                inserts
                    .entry(RelValue::from(row))
                    // Row already inserted?
                    // Increment its multiplicity.
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
                Ok(())
            })
            .unwrap();

            plan.for_each_delete(&tx, metrics, &mut |row| {
                let row = RelValue::from(row);
                match inserts.get_mut(&row) {
                    // This row was not inserted.
                    // Add it to the delete set.
                    None => {
                        deletes.push(row);
                    }
                    // This row was inserted.
                    // Decrement the multiplicity.
                    Some(1) => {
                        inserts.remove(&row);
                    }
                    // This row was inserted.
                    // Decrement the multiplicity.
                    Some(n) => {
                        *n -= 1;
                    }
                }
                Ok(())
            })
            .unwrap();

            UpdatesRelValue {
                inserts: inserts.into_keys().collect(),
                deletes,
            }
        };

        let updates = eval_delta();

        let inserts = updates
            .inserts
            .into_iter()
            .map(RelValue::into_product_value)
            .collect::<Arc<_>>();
        let deletes = updates
            .deletes
            .into_iter()
            .map(RelValue::into_product_value)
            .collect::<Arc<_>>();

        let tables = if inserts.is_empty() && deletes.is_empty() {
            vec![]
        } else {
            vec![DatabaseTableUpdate {
                table_id,
                table_name,
                inserts,
                deletes,
            }]
        };
        Ok(DatabaseUpdate { tables })
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

        // No updates to report
        assert!(result.is_empty());

        // The lhs row must always probe the rhs index.
        // The rhs row passes the rhs filter,
        // resulting in a probe of the rhs index.
        assert_eq!(metrics.index_seeks, 2);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

        // No updates to report
        assert!(result.is_empty());

        // The lhs row must always probe the rhs index.
        // The rhs row doesn't pass the rhs filter,
        // hence it doesn't survive to probe the lhs index.
        assert_eq!(metrics.index_seeks, 0);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

        // A single delete from lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));

        // One row passes the rhs filter, the other does not.
        // This results in a single probe of the lhs index.
        assert_eq!(metrics.index_seeks, 1);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(db, &mut metrics, &query, vec![(rhs_id, r1, false), (rhs_id, r2, true)])?;

        // A single insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(3, 8)));

        // One row passes the rhs filter, the other does not.
        // This results in a single probe of the lhs index.
        assert_eq!(metrics.index_seeks, 1);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(
            db,
            &mut metrics,
            &query,
            vec![(lhs_id, lhs_row, true), (rhs_id, rhs_row, true)],
        )?;

        // A single insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(5, 10)));

        // Because we only have inserts, only 3 delta queries are evaluated,
        // each one an index join, and each one probing the join index exactly once.
        assert_eq!(metrics.index_seeks, 3);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(
            db,
            &mut metrics,
            &query,
            vec![(lhs_id, lhs_row, true), (rhs_id, rhs_row, true)],
        )?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);

        // Because we only have inserts, only 3 delta queries are evaluated,
        // each one an index join, and each one probing the join index at most once.
        //
        // The lhs row always probes the rhs index,
        // but the rhs row doesn't pass the rhs filter,
        // hence it doesn't survive to probe the lhs index.
        assert_eq!(metrics.index_seeks, 2);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(
            db,
            &mut metrics,
            &query,
            vec![(lhs_id, lhs_row, false), (rhs_id, rhs_row, false)],
        )?;

        // A single delete from lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));

        // Because we only have inserts, only 3 delta queries are evaluated,
        // each one an index join, and each one probing the join index exactly once.
        assert_eq!(metrics.index_seeks, 3);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(
            db,
            &mut metrics,
            &query,
            vec![(lhs_id, lhs_row, false), (rhs_id, rhs_row, false)],
        )?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);

        // Because we only have inserts, only 3 delta queries are evaluated,
        // each one an index join, and each one probing the join index at most once.
        //
        // The lhs row always probes the rhs index,
        // but the rhs row doesn't pass the rhs filter,
        // hence it doesn't survive to probe the lhs index.
        assert_eq!(metrics.index_seeks, 2);
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

        let mut metrics = ExecutionMetrics::default();

        let result = eval_incr(
            db,
            &mut metrics,
            &query,
            vec![
                (lhs_id, lhs_old, false),
                (rhs_id, rhs_old, false),
                (lhs_id, lhs_new, true),
                (rhs_id, rhs_new, true),
            ],
        )?;

        let lhs_old = product!(1, 6);
        let lhs_new = product!(1, 7);

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

        // Because we have deletes and inserts for both tables,
        // all 8 delta queries are evaluated,
        // each one probing the join index exactly once.
        assert_eq!(metrics.index_seeks, 8);
        Ok(())
    }
}
