use crate::db::db_metrics::{DB_METRICS, MAX_QUERY_COMPILE_TIME};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::WorkloadType;
use crate::host::module_host::DatabaseTableUpdate;
use crate::sql::compiler::compile_sql;
use crate::subscription::subscription::SupportedQuery;
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Address;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::db::auth::StAccess;
use spacetimedb_sats::relation::{Column, FieldName, Header};
use spacetimedb_sats::AlgebraicType;
use spacetimedb_vm::expr::{self, Crud, CrudExpr, DbType, QueryExpr, SourceSet};
use spacetimedb_vm::relation::MemTable;
use std::sync::Arc;
use std::time::Instant;

use super::subscription::get_all;

static WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
pub const SUBSCRIBE_TO_ALL_QUERY: &str = "SELECT * FROM *";

pub enum QueryDef {
    Table(String),
    Sql(String),
}

pub const OP_TYPE_FIELD_NAME: &str = "__op_type";

/// Locate the `__op_type` column in the table described by `header`,
/// if it exists.
///
/// The current version of this function depends on the fact that
/// the `__op_type` column is always the final column in the schema.
/// This is true because the `__op_type` column is added by [`to_mem_table_with_op_type`] at the end,
/// and never originates anywhere else.
///
/// If we ever change to having the `__op_type` column in any other position,
/// e.g. by projecting together two `MemTables` from [`to_mem_table_with_op_type`],
/// this function may need to change, possibly to:
/// ```ignore
/// header.find_pos_by_name(OP_TYPE_FIELD_NAME)
/// ```
pub fn find_op_type_col_pos(header: &Header) -> Option<ColId> {
    if let Some(last_col) = header.fields.last() {
        if last_col.field.field_name() == Some(OP_TYPE_FIELD_NAME) {
            return Some(ColId((header.fields.len() - 1) as u32));
        }
    }
    None
}

/// Create a virtual table from a sequence of table updates.
/// Add a special column __op_type to distinguish inserts and deletes.
pub fn to_mem_table_with_op_type(head: Arc<Header>, table_access: StAccess, data: &DatabaseTableUpdate) -> MemTable {
    let mut t = MemTable::new(head, table_access, Vec::with_capacity(data.ops.len()));

    if let Some(pos) = find_op_type_col_pos(&t.head) {
        for op in data.ops.iter().map(|row| {
            let mut new = row.row.clone();

            match new.elements.len().cmp(&pos.idx()) {
                std::cmp::Ordering::Equal => {
                    // When we enter through `ExecutionUnit::eval_incr`,
                    // we will have a `head` computed by a previous call to `to_mem_table`,
                    // and therefore will have an op_type column,
                    // but the `data` will be fresh for a newly committed transaction,
                    // and therefore the rows will not include the op_type column.
                    // In that case, push the op_type onto the end of each row.
                    new.elements.push(row.op_type.into());
                }
                std::cmp::Ordering::Greater => {
                    new.elements[pos.idx()] = row.op_type.into();
                }
                std::cmp::Ordering::Less => {
                    panic!(
                        "Expected {} either in-bounds or as the last column, but found at position {} in {:?}",
                        OP_TYPE_FIELD_NAME, pos, t.head,
                    );
                }
            }

            new
        }) {
            t.data.push(op);
        }
    } else {
        // TODO(perf): Eliminate this `clone_for_error` call, as we're not in an error path.
        let mut head = t.head.clone_for_error();
        head.fields.push(Column::new(
            FieldName::named(&t.head.table_name, OP_TYPE_FIELD_NAME),
            AlgebraicType::U8,
            t.head.fields.len().into(),
        ));
        t.head = Arc::new(head);
        for row in &data.ops {
            let mut new = row.row.clone();
            new.elements.push(row.op_type.into());
            t.data.push(new);
        }
    }
    t
}

/// Replace the primary (ie. `source`) table of the given [`QueryExpr`] with
/// a virtual [`MemTable`] consisting of the rows in [`DatabaseTableUpdate`].
///
/// To be able to reify the `op_type` of the individual operations in the update,
/// each virtual row is extended with a column [`OP_TYPE_FIELD_NAME`].
pub fn to_mem_table(mut of: QueryExpr, data: &DatabaseTableUpdate) -> (QueryExpr, SourceSet) {
    let mem_table = to_mem_table_with_op_type(of.source.head().clone(), of.source.table_access(), data);
    let mut sources = SourceSet::default();
    let source_expr = sources.add_mem_table(mem_table);
    of.source = source_expr;
    (of, sources)
}

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
    auth: &AuthCtx,
    input: &str,
) -> Result<Vec<SupportedQuery>, DBError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SubscriptionError::Empty.into());
    }

    // Remove redundant whitespace, and in particular newlines, for debug info.
    let input = WHITESPACE.replace_all(input, " ");
    if input == SUBSCRIBE_TO_ALL_QUERY {
        return get_all(relational_db, tx, auth);
    }

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

// TODO: Enable query compilation metrics once cardinality has been addressed.
#[allow(unused)]
fn record_query_compilation_metrics(workload: WorkloadType, db: &Address, query: &str, start: Instant) {
    let compile_duration = start.elapsed().as_secs_f64();

    DB_METRICS
        .rdb_query_compile_time_sec
        .with_label_values(&workload, db)
        .observe(compile_duration);

    let max_compile_duration = *MAX_QUERY_COMPILE_TIME
        .lock()
        .unwrap()
        .entry((*db, workload))
        .and_modify(|max| {
            if compile_duration > *max {
                *max = compile_duration;
            }
        })
        .or_insert_with(|| compile_duration);

    DB_METRICS
        .rdb_query_compile_time_sec_max
        .with_label_values(&workload, db)
        .set(max_compile_duration);
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
    if expr.query.len() == 1 && matches!(expr.query[0], IndexJoin(_)) {
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
    use std::sync::Arc;

    use super::*;
    use crate::db::datastore::traits::IsolationLevel;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::db::relational_db::MutTx;
    use crate::execution_context::ExecutionContext;
    use crate::host::module_host::{DatabaseUpdate, TableOp};
    use crate::sql::execute::run;
    use crate::subscription::subscription::ExecutionSet;
    use crate::vm::tests::create_table_with_rows;
    use itertools::Itertools;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::Identity;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::*;
    use spacetimedb_sats::relation::FieldName;
    use spacetimedb_sats::{product, ProductType, ProductValue};
    use spacetimedb_vm::dsl::{mem_table, scalar};
    use spacetimedb_vm::operator::OpCmp;

    /// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
    fn run_query(
        cx: &ExecutionContext,
        db: &RelationalDB,
        tx: &Tx,
        query: &QueryExpr,
        auth: AuthCtx,
        sources: SourceSet,
    ) -> Result<Vec<MemTable>, DBError> {
        crate::sql::execute::execute_single_sql(cx, db, tx, CrudExpr::Query(query.clone()), auth, sources)
    }

    fn insert_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
        DatabaseTableUpdate {
            table_id,
            table_name: table_name.to_string(),
            ops: vec![TableOp::insert(row)],
        }
    }

    fn delete_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
        DatabaseTableUpdate {
            table_id,
            table_name: table_name.to_string(),
            ops: vec![TableOp::delete(row)],
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
    ) -> ResultTest<(TableSchema, MemTable, DatabaseTableUpdate, QueryExpr)> {
        let table = mem_table(head.clone(), [row.clone()]);
        let table_id = create_table_with_rows(db, tx, table_name, head.clone(), &[row.clone()])?;

        let data = DatabaseTableUpdate {
            table_id,
            table_name: table_name.to_string(),
            ops: vec![TableOp::insert(row.clone())],
        };

        let schema = db.schema_for_table_mut(tx, table_id).unwrap().into_owned();

        let q = QueryExpr::new(&schema);

        Ok((schema, table, data, q))
    }

    fn make_inv(
        db: &RelationalDB,
        tx: &mut MutTx,
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
        tx: &mut MutTx,
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
        tx: &Tx,
        q: &QueryExpr,
        data: &DatabaseTableUpdate,
    ) -> ResultTest<()> {
        let (q, sources) = to_mem_table(q.clone(), data);
        let result = run_query(
            &ExecutionContext::default(),
            db,
            tx,
            &q,
            AuthCtx::for_testing(),
            sources,
        )?;

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
        tx: &Tx,
        s: &ExecutionSet,
        update: &DatabaseUpdate,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let result = s.eval_incr(db, tx, update)?;
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
        tx: &Tx,
        s: &ExecutionSet,
        total_tables: usize,
        rows: &[ProductValue],
    ) -> ResultTest<()> {
        let result = s.eval(db, tx)?;
        assert_eq!(
            result.tables.len(),
            total_tables,
            "Must return the correct number of tables: {result:#?}"
        );

        let result = get_result(result);

        assert_eq!(result, rows, "Must return the correct row(s)");

        Ok(())
    }

    fn singleton_execution_set(expr: QueryExpr) -> ResultTest<ExecutionSet> {
        Ok(ExecutionSet::from_iter([SupportedQuery::try_from(expr)?]))
    }

    #[test]
    fn test_eval_incr_for_index_scan() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let table_id = db.create_table_for_test("test", schema, indexes)?;

        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let mut ops = Vec::new();
        for i in 0u64..9u64 {
            let row = product!(i, i);
            db.insert(&mut tx, table_id, row)?;

            let row = product!(i + 10, i);
            ops.push(TableOp::delete(row))
        }

        let update = DatabaseUpdate {
            tables: vec![DatabaseTableUpdate {
                table_id,
                table_name: "test".into(),
                ops,
            }],
        };

        db.commit_tx(&ExecutionContext::default(), tx)?;
        let tx = db.begin_tx();

        let sql = "select * from test where b = 3";
        let mut exp = compile_sql(&db, &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query: ExecutionSet = singleton_execution_set(query)?;

        let result = query.eval_incr(&db, &tx, &update)?;

        assert_eq!(result.tables.len(), 1);

        let update = &result.tables[0];

        assert_eq!(update.ops.len(), 1);

        let op = &update.ops[0];

        assert_eq!(op.op_type, 0);
        assert_eq!(op.row, product!(13u64, 3u64));
        Ok(())
    }

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Public)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Public);

        let tx = db.begin_tx();
        let q_1 = q.clone();
        check_query(&db, &table, &tx, &q_1, &data)?;

        let q_2 = q.with_select_cmp(OpCmp::Eq, FieldName::named("inventory", "inventory_id"), scalar(1u64));
        check_query(&db, &table, &tx, &q_2, &data)?;

        Ok(())
    }

    // Check that the `owner` can access private tables (that start with `_`) and that it fails if the `caller` is different
    #[test]
    fn test_subscribe_private() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Private)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Private);

        let row = product!(1u64, "health");
        let tx = db.begin_tx();
        check_query(&db, &table, &tx, &q, &data)?;

        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id = QueryExpr::new(&schema).with_select_cmp(
            OpCmp::Eq,
            FieldName::named("_inventory", "inventory_id"),
            scalar(1u64),
        );

        let s = singleton_execution_set(q_id)?;

        let row2 = TableOp::insert(row.clone());

        let data = DatabaseTableUpdate {
            table_id: schema.table_id,
            table_name: "_inventory".to_string(),
            ops: vec![row2],
        };

        let update = DatabaseUpdate {
            tables: vec![data.clone()],
        };

        check_query_incr(&db, &tx, &s, &update, 1, &[row])?;

        let q = QueryExpr::new(&schema);

        let (q, sources) = to_mem_table(q, &data);
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
        let (db, _tmp_dir) = make_test_db()?;

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
        run(&db, sql_insert, AuthCtx::for_testing())?;

        let sql_query = "\
            SELECT EnemyState.* FROM EnemyState \
            JOIN MobileEntityState ON MobileEntityState.entity_id = EnemyState.entity_id  \
            WHERE MobileEntityState.location_x > 96000 \
            AND MobileEntityState.location_x < 192000 \
            AND MobileEntityState.location_z > 96000 \
            AND MobileEntityState.location_z < 192000";

        let tx = db.begin_tx();
        let qset = compile_read_only_query(&db, &tx, &AuthCtx::for_testing(), sql_query)?;

        for q in qset {
            let result = run_query(
                &ExecutionContext::default(),
                &db,
                &tx,
                q.as_expr(),
                AuthCtx::for_testing(),
                SourceSet::default(),
            )?;
            assert_eq!(result.len(), 1, "Join query did not return any rows");
        }

        Ok(())
    }

    #[test]
    fn test_subscribe_all() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        let (schema_1, _, _, _) = make_inv(&db, &mut tx, StAccess::Public)?;
        let (schema_2, _, _, _) = make_player(&db, &mut tx)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");
        let tx = db.begin_tx();
        let s = compile_read_only_query(&db, &tx, &AuthCtx::for_testing(), SUBSCRIBE_TO_ALL_QUERY)?.into();
        check_query_eval(&db, &tx, &s, 2, &[row_1.clone(), row_2.clone()])?;

        let data1 = DatabaseTableUpdate {
            table_id: schema_1.table_id,
            table_name: "inventory".to_string(),
            ops: vec![TableOp::delete(row_1)],
        };

        let data2 = DatabaseTableUpdate {
            table_id: schema_2.table_id,
            table_name: "player".to_string(),
            ops: vec![TableOp::insert(row_2)],
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
        let (db, _tmp_dir) = make_test_db()?;

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
            let expr = compile_read_only_query(&db, &tx, &auth, scan)?.pop().unwrap();
            assert_eq!(expr.kind(), Supported::Select, "{scan}\n{expr:#?}");
        }

        // Only index semijoins are supported
        let joins = ["SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE rhs.y < 10"];
        for join in joins {
            let expr = compile_read_only_query(&db, &tx, &auth, join)?.pop().unwrap();
            assert_eq!(expr.kind(), Supported::Semijoin, "{join}\n{expr:#?}");
        }

        // All other joins are unsupported
        let joins = [
            "SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id",
            "SELECT * FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE lhs.x < 10",
        ];
        for join in joins {
            match compile_read_only_query(&db, &tx, &auth, join) {
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
            singleton_execution_set(query)
        })
    }

    fn run_eval_incr_test<T, F: Fn(RelationalDB) -> ResultTest<T>>(test_fn: F) -> ResultTest<T> {
        make_test_db().map(|(db, _)| test_fn(db))??;
        make_test_db().map(|(db, _)| test_fn(db.with_row_count(Arc::new(|_, _| 5))))?
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
        Ok(())
    }

    // Case 1:
    // Delete a row inside the region of rhs,
    // Insert a row inside the region of rhs.
    fn index_join_case_1(db: RelationalDB) -> ResultTest<()> {
        let _ = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let r1 = product!(10, 0, 2);
        let r2 = product!(10, 0, 3);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(&db, tx, rhs_id, r1.clone());
            insert_row(&db, tx, rhs_id, r2.clone())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        delete_op(rhs_id, "rhs", r1.clone()),
                        insert_op(rhs_id, "rhs", r2.clone()),
                    ],
                },
            )
        })?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 2:
    // Delete a row outside the region of rhs,
    // Insert a row outside the region of rhs.
    fn index_join_case_2(db: RelationalDB) -> ResultTest<()> {
        let _ = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let r1 = product!(13, 3, 5);
        let r2 = product!(13, 3, 6);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(&db, tx, rhs_id, r1.clone());
            insert_row(&db, tx, rhs_id, r2.clone())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        delete_op(rhs_id, "rhs", r1.clone()),
                        insert_op(rhs_id, "rhs", r2.clone()),
                    ],
                },
            )
        })?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 3:
    // Delete a row inside  the region of rhs,
    // Insert a row outside the region of rhs.
    fn index_join_case_3(db: RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let r1 = product!(10, 0, 2);
        let r2 = product!(10, 0, 5);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(&db, tx, rhs_id, r1.clone());
            insert_row(&db, tx, rhs_id, r2.clone())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        delete_op(rhs_id, "rhs", r1.clone()),
                        insert_op(rhs_id, "rhs", r2.clone()),
                    ],
                },
            )
        })?;

        // A single delete from lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));
        Ok(())
    }

    // Case 4:
    // Delete a row outside the region of rhs,
    // Insert a row inside  the region of rhs.
    fn index_join_case_4(db: RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let r1 = product!(13, 3, 5);
        let r2 = product!(13, 3, 4);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            delete_row(&db, tx, rhs_id, r1.clone());
            insert_row(&db, tx, rhs_id, r2.clone())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        delete_op(rhs_id, "rhs", r1.clone()),
                        insert_op(rhs_id, "rhs", r2.clone()),
                    ],
                },
            )
        })?;

        // A single insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(3, 8)));
        Ok(())
    }

    // Case 5:
    // Insert row into lhs,
    // Insert matching row inside the region of rhs.
    fn index_join_case_5(db: RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let lhs_row = product!(5, 10);
        let rhs_row = product!(20, 5, 3);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            insert_row(&db, tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, tx, rhs_id, rhs_row.clone())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        insert_op(lhs_id, "lhs", lhs_row.clone()),
                        insert_op(rhs_id, "rhs", rhs_row.clone()),
                    ],
                },
            )
        })?;

        // A single insert into lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(5, 10)));
        Ok(())
    }

    // Case 6:
    // Insert row into lhs,
    // Insert matching row outside the region of rhs.
    fn index_join_case_6(db: RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let lhs_row = product!(5, 10);
        let rhs_row = product!(20, 5, 5);

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            insert_row(&db, tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, tx, rhs_id, rhs_row.clone())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        insert_op(lhs_id, "lhs", lhs_row.clone()),
                        insert_op(rhs_id, "rhs", rhs_row.clone()),
                    ],
                },
            )
        })?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }

    // Case 7:
    // Delete row from lhs,
    // Delete matching row inside the region of rhs.
    fn index_join_case_7(db: RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let lhs_row = product!(0, 5);
        let rhs_row = product!(10, 0, 2);

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> ResultTest<_> {
            delete_row(&db, tx, lhs_id, lhs_row.clone());
            delete_row(&db, tx, rhs_id, rhs_row.clone());
            Ok(())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        delete_op(lhs_id, "lhs", lhs_row.clone()),
                        delete_op(rhs_id, "rhs", rhs_row.clone()),
                    ],
                },
            )
        })?;

        // A single delete from lhs
        assert_eq!(result.tables.len(), 1);
        assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));
        Ok(())
    }

    // Case 8:
    // Delete row from lhs,
    // Delete matching row outside the region of rhs.
    fn index_join_case_8(db: RelationalDB) -> ResultTest<()> {
        let lhs_id = create_lhs_table_for_eval_incr(&db)?;
        let rhs_id = create_rhs_table_for_eval_incr(&db)?;
        let query = compile_query(&db)?;

        let lhs_row = product!(3, 8);
        let rhs_row = product!(13, 3, 5);

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> ResultTest<_> {
            delete_row(&db, tx, lhs_id, lhs_row.clone());
            delete_row(&db, tx, rhs_id, rhs_row.clone());
            Ok(())
        })?;

        let result = db.with_read_only(&ExecutionContext::default(), |tx| {
            query.eval_incr(
                &db,
                tx,
                &DatabaseUpdate {
                    tables: vec![
                        delete_op(lhs_id, "lhs", lhs_row.clone()),
                        delete_op(rhs_id, "rhs", rhs_row.clone()),
                    ],
                },
            )
        })?;

        // No updates to report
        assert_eq!(result.tables.len(), 0);
        Ok(())
    }
}
