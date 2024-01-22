use std::time::Instant;

use crate::db::db_metrics::{DB_METRICS, MAX_QUERY_COMPILE_TIME};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::{ExecutionContext, WorkloadType};
use crate::host::module_host::DatabaseTableUpdate;
use crate::sql::compiler::compile_sql;
use crate::sql::execute::execute_single_sql;
use crate::subscription::subscription::{QuerySet, SupportedQuery};
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Address;
use spacetimedb_sats::db::auth::StAccess;
use spacetimedb_sats::relation::{Column, FieldName, Header, MemTable, RelValue};
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::DataKey;
use spacetimedb_vm::expr;
use spacetimedb_vm::expr::{Crud, CrudExpr, DbType, QueryExpr};

static WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
pub const SUBSCRIBE_TO_ALL_QUERY: &str = "SELECT * FROM *";

pub enum QueryDef {
    Table(String),
    Sql(String),
}

pub const OP_TYPE_FIELD_NAME: &str = "__op_type";

/// Create a virtual table from a sequence of table updates.
/// Add a special column __op_type to distinguish inserts and deletes.
#[tracing::instrument(skip_all)]
pub fn to_mem_table_with_op_type(head: Header, table_access: StAccess, data: &DatabaseTableUpdate) -> MemTable {
    let mut t = MemTable::new(head, table_access, vec![]);

    if let Some(pos) = t.head.find_pos_by_name(OP_TYPE_FIELD_NAME) {
        t.data.extend(data.ops.iter().map(|row| {
            let mut new = row.row.clone();
            new.elements[pos.idx()] = row.op_type.into();
            let mut bytes: &[u8] = row.row_pk.as_ref();
            RelValue::new(new, Some(DataKey::decode(&mut bytes).unwrap()))
        }));
    } else {
        t.head.fields.push(Column::new(
            FieldName::named(&t.head.table_name, OP_TYPE_FIELD_NAME),
            AlgebraicType::U8,
            t.head.fields.len().into(),
        ));
        for row in &data.ops {
            let mut new = row.row.clone();
            new.elements.push(row.op_type.into());
            let mut bytes: &[u8] = row.row_pk.as_ref();
            t.data
                .push(RelValue::new(new, Some(DataKey::decode(&mut bytes).unwrap())));
        }
    }
    t
}

/// Replace the primary (ie. `source`) table of the given [`QueryExpr`] with
/// a virtual [`MemTable`] consisting of the rows in [`DatabaseTableUpdate`].
///
/// To be able to reify the `op_type` of the individual operations in the update,
/// each virtual row is extended with a column [`OP_TYPE_FIELD_NAME`].
pub fn to_mem_table(mut of: QueryExpr, data: &DatabaseTableUpdate) -> QueryExpr {
    of.source = to_mem_table_with_op_type(of.source.head().clone(), of.source.table_access(), data).into();
    of
}

/// Runs a query that evaluates if the changes made should be reported to the [ModuleSubscriptionManager]
#[tracing::instrument(skip_all)]
pub(crate) fn run_query(
    cx: &ExecutionContext,
    db: &RelationalDB,
    tx: &mut Tx,
    query: &QueryExpr,
    auth: AuthCtx,
) -> Result<Vec<MemTable>, DBError> {
    execute_single_sql(cx, db, tx, CrudExpr::Query(query.clone()), auth)
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
    tx: &Tx,
    auth: &AuthCtx,
    input: &str,
) -> Result<QuerySet, DBError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SubscriptionError::Empty.into());
    }

    // Remove redundant whitespace, and in particular newlines, for debug info.
    let input = WHITESPACE.replace_all(input, " ");
    if input == SUBSCRIBE_TO_ALL_QUERY {
        return QuerySet::get_all(relational_db, tx, auth);
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
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Supported {
    /// A scan or [`QueryExpr::Select`] of a single table.
    Scan,
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
    Some(Supported::Scan)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::db::relational_db::MutTx;
    use crate::host::module_host::{DatabaseUpdate, TableOp};
    use crate::sql::execute::run;
    use crate::vm::tests::create_table_with_rows;
    use itertools::Itertools;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::Identity;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::data_key::ToDataKey;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::*;
    use spacetimedb_sats::relation::FieldName;
    use spacetimedb_sats::{product, ProductType, ProductValue};
    use spacetimedb_vm::dsl::{db_table, mem_table, scalar};
    use spacetimedb_vm::operator::OpCmp;

    fn create_table(
        db: &RelationalDB,
        tx: &mut MutTx,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(ColId, &str)],
    ) -> ResultTest<TableId> {
        let table_name = name.to_string();
        let table_type = StTableType::User;
        let table_access = StAccess::Public;

        let columns = schema
            .iter()
            .map(|(col_name, col_type)| ColumnDef {
                col_name: col_name.to_string(),
                col_type: col_type.clone(),
            })
            .collect_vec();

        let indexes = indexes
            .iter()
            .map(|(col_id, index_name)| IndexDef::btree(index_name.to_string(), *col_id, false))
            .collect_vec();

        let schema = TableDef::new(table_name, columns)
            .with_indexes(indexes)
            .with_type(table_type)
            .with_access(table_access);

        Ok(db.create_table(tx, schema)?)
    }

    fn insert_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
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

    fn delete_op(table_id: TableId, table_name: &str, row: ProductValue) -> DatabaseTableUpdate {
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

        let schema = db.schema_for_table_mut(tx, table_id).unwrap().into_owned();

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

        let q = QueryExpr::new(db_table(&schema, table_id));

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
        tx: &mut Tx,
        q: &QueryExpr,
        data: &DatabaseTableUpdate,
    ) -> ResultTest<()> {
        let q = to_mem_table(q.clone(), data);
        let result = run_query(&ExecutionContext::default(), db, tx, &q, AuthCtx::for_testing())?;

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
        tx: &mut Tx,
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
        tx: &mut Tx,
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
        let mut tx = db.begin_mut_tx();

        let schema = ProductType::from([("u8", AlgebraicType::U8)]);
        let row = product!(1u8);

        // generate row id from row
        let id1 = &row.to_data_key().to_bytes();

        // create table empty table "test"
        let table_id = create_table_with_rows(&db, &mut tx, "test", schema.clone(), &[])?;

        // select * from test
        let query: QuerySet = QueryExpr::new(db_table(schema.clone(), table_id)).try_into()?;

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
        db.rollback_mut_tx(&ExecutionContext::default(), tx);
        let mut tx = db.begin_tx();
        let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
        let id2 = &result.tables[0].ops[0].row_pk;

        // check that both row ids are the same
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn test_eval_incr_for_index_scan() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let table_id = create_table(&db, &mut tx, "test", schema, indexes)?;

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

        db.commit_tx(&ExecutionContext::default(), tx)?;
        let mut tx = db.begin_tx();

        let sql = "select * from test where b = 3";
        let mut exp = compile_sql(&db, &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query = QuerySet::try_from(query)?;

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
        let (db, _tmp) = make_test_db()?;
        run_eval_incr_for_index_join(db)?;

        let (db, _tmp) = make_test_db()?;
        run_eval_incr_for_index_join(db.with_row_count(Arc::new(|_, _| 5)))?;
        Ok(())
    }

    fn run_eval_incr_for_index_join(db: RelationalDB) -> ResultTest<()> {
        let mut tx = db.begin_mut_tx();

        // Create table [lhs] with index on [id]
        let schema = &[("id", AlgebraicType::I32), ("x", AlgebraicType::I32)];
        let indexes = &[(0.into(), "id")];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with index on [id]
        let schema = &[
            ("rid", AlgebraicType::I32),
            ("id", AlgebraicType::I32),
            ("y", AlgebraicType::I32),
        ];
        let indexes = &[(1.into(), "id")];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, indexes)?;

        // Insert into lhs
        for i in 0..5 {
            db.insert(&mut tx, lhs_id, product!(i, i + 5))?;
        }

        // Insert into rhs
        for i in 10..20 {
            db.insert(&mut tx, rhs_id, product!(i, i - 10, i - 8))?;
        }
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should be answered using an index semijion
        let sql = "select lhs.* from lhs join rhs on lhs.id = rhs.id where rhs.y >= 2 and rhs.y <= 4";
        let mut exp = compile_sql(&db, &tx, sql)?;

        let Some(CrudExpr::Query(query)) = exp.pop() else {
            panic!("unexpected query {:#?}", exp[0]);
        };

        let query = QuerySet::try_from(query)?;
        db.release_tx(&ExecutionContext::default(), tx);

        fn case_env(
            db: &RelationalDB,
            rhs_id: TableId,
            del_row: ProductValue,
            ins_row: ProductValue,
        ) -> ResultTest<()> {
            let mut tx = db.begin_mut_tx();
            delete_row(db, &mut tx, rhs_id, del_row);
            insert_row(db, &mut tx, rhs_id, ins_row)?;
            db.commit_tx(&ExecutionContext::default(), tx)?;
            Ok(())
        }

        // Case 1: Delete a row inside the region and insert back inside the region
        {
            let r1 = product!(10, 0, 2);
            let r2 = product!(10, 0, 3);
            case_env(&db, rhs_id, r1.clone(), r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];
            let mut tx = db.begin_tx();
            let update = DatabaseUpdate { tables: updates };
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            case_env(&db, rhs_id, r2.clone(), r1.clone())?;
        }

        // Case 2: Delete a row outside the region and insert back outside the region
        {
            let r1 = product!(13, 3, 5);
            let r2 = product!(13, 3, 6);

            case_env(&db, rhs_id, r2.clone(), r1.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            case_env(&db, rhs_id, r1.clone(), r2.clone())?;
        }

        // Case 3: Delete a row inside the region and insert back outside the region
        {
            let r1 = product!(10, 0, 2);
            let r2 = product!(10, 0, 5);

            case_env(&db, rhs_id, r1.clone(), r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // A single delete from lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));

            // Clean up tx
            case_env(&db, rhs_id, r2.clone(), r1.clone())?;
        }

        // Case 4: Delete a row outside the region and insert back inside the region
        {
            let r1 = product!(13, 3, 5);
            let r2 = product!(13, 3, 4);

            case_env(&db, rhs_id, r1.clone(), r2.clone())?;

            let updates = vec![
                delete_op(rhs_id, "rhs", r1.clone()),
                insert_op(rhs_id, "rhs", r2.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // A single insert into lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(3, 8)));

            // Clean up tx
            case_env(&db, rhs_id, r2.clone(), r1.clone())?;
        }

        // Case 5: Insert a row into lhs and insert a matching row inside the region of rhs
        {
            let lhs_row = product!(5, 10);
            let rhs_row = product!(20, 5, 3);
            let mut tx = db.begin_mut_tx();
            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
            db.commit_tx(&ExecutionContext::default(), tx)?;

            let updates = vec![
                insert_op(lhs_id, "lhs", lhs_row.clone()),
                insert_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // A single insert into lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], insert_op(lhs_id, "lhs", product!(5, 10)));

            // Clean up tx
            let mut tx = db.begin_mut_tx();
            delete_row(&db, &mut tx, lhs_id, lhs_row.clone());
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone());
            db.commit_tx(&ExecutionContext::default(), tx)?;
        }

        // Case 6: Insert a row into lhs and insert a matching row outside the region of rhs
        {
            let lhs_row = product!(5, 10);
            let rhs_row = product!(20, 5, 5);
            let mut tx = db.begin_mut_tx();
            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
            db.commit_tx(&ExecutionContext::default(), tx)?;

            let updates = vec![
                insert_op(lhs_id, "lhs", lhs_row.clone()),
                insert_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            let mut tx = db.begin_mut_tx();
            delete_row(&db, &mut tx, lhs_id, lhs_row.clone());
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone());
            db.commit_tx(&ExecutionContext::default(), tx)?;
        }

        // Case 7: Delete a row from lhs and delete a matching row inside the region of rhs
        {
            let lhs_row = product!(0, 5);
            let rhs_row = product!(10, 0, 2);
            let mut tx = db.begin_mut_tx();
            delete_row(&db, &mut tx, lhs_id, lhs_row.clone());
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone());
            db.commit_tx(&ExecutionContext::default(), tx)?;

            let updates = vec![
                delete_op(lhs_id, "lhs", lhs_row.clone()),
                delete_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // A single delete from lhs
            assert_eq!(result.tables.len(), 1);
            assert_eq!(result.tables[0], delete_op(lhs_id, "lhs", product!(0, 5)));

            // Clean up tx
            let mut tx = db.begin_mut_tx();
            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
            db.commit_tx(&ExecutionContext::default(), tx)?;
        }

        // Case 8: Delete a row from lhs and delete a matching row outside the region of rhs
        {
            let lhs_row = product!(3, 8);
            let rhs_row = product!(13, 3, 5);
            let mut tx = db.begin_mut_tx();
            delete_row(&db, &mut tx, lhs_id, lhs_row.clone());
            delete_row(&db, &mut tx, rhs_id, rhs_row.clone());
            db.commit_tx(&ExecutionContext::default(), tx)?;

            let updates = vec![
                delete_op(lhs_id, "lhs", lhs_row.clone()),
                delete_op(rhs_id, "rhs", rhs_row.clone()),
            ];

            let update = DatabaseUpdate { tables: updates };
            let mut tx = db.begin_tx();
            let result = query.eval_incr(&db, &mut tx, &update, AuthCtx::for_testing())?;
            db.release_tx(&ExecutionContext::default(), tx);

            // No updates to report
            assert_eq!(result.tables.len(), 0);

            // Clean up tx
            let mut tx = db.begin_mut_tx();
            insert_row(&db, &mut tx, lhs_id, lhs_row.clone())?;
            insert_row(&db, &mut tx, rhs_id, rhs_row.clone())?;
            db.commit_tx(&ExecutionContext::default(), tx)?;
        }

        Ok(())
    }

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Public)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Public);

        let mut tx = db.begin_tx();
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
        let mut tx = db.begin_mut_tx();

        let (schema, table, data, q) = make_inv(&db, &mut tx, StAccess::Private)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        assert_eq!(schema.table_type, StTableType::User);
        assert_eq!(schema.table_access, StAccess::Private);

        let row = product!(1u64, "health");
        let mut tx = db.begin_tx();
        check_query(&db, &table, &mut tx, &q, &data)?;

        //SELECT * FROM inventory
        let q_all = QueryExpr::new(db_table(&schema, schema.table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("_inventory", "inventory_id"), scalar(1u64));

        let s = [q_all, q_id]
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<QuerySet, _>>()?;

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

        let q = QueryExpr::new(db_table(&schema, schema.table_id));

        let q = to_mem_table(q, &data);
        //Try access the private table
        match run_query(
            &ExecutionContext::default(),
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
        let mut tx = db.begin_mut_tx();

        let (schema, _table, _data, _q) = make_inv(&db, &mut tx, StAccess::Private)?;

        //SELECT * FROM inventory
        let q_all = QueryExpr::new(db_table(&schema, schema.table_id));
        //SELECT * FROM inventory WHERE inventory_id = 1
        let q_id =
            q_all
                .clone()
                .with_select_cmp(OpCmp::Eq, FieldName::named("_inventory", "inventory_id"), scalar(1u64));

        let s = [q_all, q_id]
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<QuerySet, _>>()?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let mut tx = db.begin_tx();
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
        let mut tx = db.begin_mut_tx();

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
        create_table(&db, &mut tx, "MobileEntityState", schema, indexes)?;

        // Create table [EnemyState]
        let schema = &[
            ("entity_id", AlgebraicType::U64),
            ("herd_id", AlgebraicType::I32),
            ("status", AlgebraicType::I32),
            ("type", AlgebraicType::I32),
            ("direction", AlgebraicType::I32),
        ];
        let indexes = &[(0.into(), "entity_id")];
        create_table(&db, &mut tx, "EnemyState", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

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

        let mut tx = db.begin_tx();
        let qset = compile_read_only_query(&db, &tx, &AuthCtx::for_testing(), sql_query)?;

        for q in qset {
            let result = run_query(
                &ExecutionContext::default(),
                &db,
                &mut tx,
                q.as_expr(),
                AuthCtx::for_testing(),
            )?;
            assert_eq!(result.len(), 1, "Join query did not return any rows");
        }

        Ok(())
    }

    #[test]
    fn test_subscribe_all() -> ResultTest<()> {
        let (db, _tmp_dir) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        let (schema_1, _, _, _) = make_inv(&db, &mut tx, StAccess::Public)?;
        let (schema_2, _, _, _) = make_player(&db, &mut tx)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;
        let row_1 = product!(1u64, "health");
        let row_2 = product!(2u64, "jhon doe");
        let mut tx = db.begin_tx();
        let s = compile_read_only_query(&db, &tx, &AuthCtx::for_testing(), SUBSCRIBE_TO_ALL_QUERY)?;
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
        let mut tx = db.begin_mut_tx();

        // Create table [plain]
        let schema = &[("id", AlgebraicType::U64)];
        create_table(&db, &mut tx, "plain", schema, &[])?;

        // Create table [lhs] with indexes on [id] and [x]
        let schema = &[("id", AlgebraicType::U64), ("x", AlgebraicType::I32)];
        let indexes = &[(ColId(0), "id"), (ColId(1), "x")];
        create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with indexes on [id] and [y]
        let schema = &[("id", AlgebraicType::U64), ("y", AlgebraicType::I32)];
        let indexes = &[(ColId(0), "id"), (ColId(1), "y")];
        create_table(&db, &mut tx, "rhs", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

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
            let expr = compile_read_only_query(&db, &tx, &auth, scan)?.pop_first().unwrap();
            assert_eq!(expr.kind(), Supported::Scan, "{scan}\n{expr:#?}");
        }

        // Only index semijoins are supported
        let joins = ["SELECT lhs.* FROM lhs JOIN rhs ON lhs.id = rhs.id WHERE rhs.y < 10"];
        for join in joins {
            let expr = compile_read_only_query(&db, &tx, &auth, join)?.pop_first().unwrap();
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
}
