use std::time::Duration;

use super::ast::SchemaViewer;
use crate::db::datastore::locking_tx_datastore::state_view::StateView;
use crate::db::datastore::traits::IsolationLevel;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::execution_context::Workload;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::ArgsTuple;
use crate::subscription::module_subscription_actor::{ModuleSubscriptions, WriteConflict};
use crate::subscription::tx::DeltaTx;
use crate::util::slow::SlowQueryLogger;
use crate::vm::{check_row_limit, DbProgram, TxMode};
use anyhow::anyhow;
use spacetimedb_expr::statement::Statement;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::relation::FieldName;
use spacetimedb_lib::Timestamp;
use spacetimedb_lib::{AlgebraicType, ProductType, ProductValue};
use spacetimedb_query::{compile_sql_stmt, execute_dml_stmt, execute_select_stmt};
use spacetimedb_vm::eval::run_ast;
use spacetimedb_vm::expr::{CodeResult, CrudExpr, Expr};
use spacetimedb_vm::relation::MemTable;

pub struct StmtResult {
    pub schema: ProductType,
    pub rows: Vec<ProductValue>,
}

// TODO(cloutiertyler): we could do this the swift parsing way in which
// we always generate a plan, but it may contain errors

pub(crate) fn collect_result(
    result: &mut Vec<MemTable>,
    updates: &mut Vec<DatabaseTableUpdate>,
    r: CodeResult,
) -> Result<(), DBError> {
    match r {
        CodeResult::Value(_) => {}
        CodeResult::Table(x) => result.push(x),
        CodeResult::Block(lines) => {
            for x in lines {
                collect_result(result, updates, x)?;
            }
        }
        CodeResult::Halt(err) => return Err(DBError::VmUser(err)),
        CodeResult::Pass(x) => match x {
            None => {}
            Some(update) => {
                updates.push(DatabaseTableUpdate {
                    table_name: update.table_name,
                    table_id: update.table_id,
                    inserts: update.inserts.into(),
                    deletes: update.deletes.into(),
                });
            }
        },
    }

    Ok(())
}

fn execute(
    p: &mut DbProgram<'_, '_>,
    ast: Vec<CrudExpr>,
    sql: &str,
    updates: &mut Vec<DatabaseTableUpdate>,
) -> Result<Vec<MemTable>, DBError> {
    let slow_query_threshold = if let TxMode::Tx(tx) = p.tx {
        p.db.query_limit(tx)?.map(Duration::from_millis)
    } else {
        None
    };
    let _slow_query_logger = SlowQueryLogger::new(sql, slow_query_threshold, p.tx.ctx().workload()).log_guard();
    let mut result = Vec::with_capacity(ast.len());
    let query = Expr::Block(ast.into_iter().map(|x| Expr::Crud(Box::new(x))).collect());
    // SQL queries can never reference `MemTable`s, so pass an empty `SourceSet`.
    collect_result(&mut result, updates, run_ast(p, query, [].into()).into())?;
    Ok(result)
}

/// Run the compiled `SQL` expression inside the `vm` created by [DbProgram]
///
/// Evaluates `ast` and accordingly triggers mutable or read tx to execute
///
/// Also, in case the execution takes more than x, log it as `slow query`
pub fn execute_sql(
    db: &RelationalDB,
    sql: &str,
    ast: Vec<CrudExpr>,
    auth: AuthCtx,
    subs: Option<&ModuleSubscriptions>,
) -> Result<Vec<MemTable>, DBError> {
    if CrudExpr::is_reads(&ast) {
        let mut updates = Vec::new();
        db.with_read_only(Workload::Sql, |tx| {
            execute(
                &mut DbProgram::new(db, &mut TxMode::Tx(tx), auth),
                ast,
                sql,
                &mut updates,
            )
        })
    } else if subs.is_none() {
        let mut updates = Vec::new();
        db.with_auto_commit(Workload::Sql, |mut_tx| {
            execute(
                &mut DbProgram::new(db, &mut mut_tx.into(), auth),
                ast,
                sql,
                &mut updates,
            )
        })
    } else {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Sql);
        let mut updates = Vec::with_capacity(ast.len());
        let res = execute(
            &mut DbProgram::new(db, &mut (&mut tx).into(), auth),
            ast,
            sql,
            &mut updates,
        );
        if res.is_ok() && !updates.is_empty() {
            let event = ModuleEvent {
                timestamp: Timestamp::now(),
                caller_identity: auth.caller,
                caller_connection_id: None,
                function_call: ModuleFunctionCall {
                    reducer: String::new(),
                    reducer_id: u32::MAX.into(),
                    args: ArgsTuple::default(),
                },
                status: EventStatus::Committed(DatabaseUpdate { tables: updates }),
                energy_quanta_used: EnergyQuanta::ZERO,
                host_execution_duration: Duration::ZERO,
                request_id: None,
                timer: None,
            };
            match subs.unwrap().commit_and_broadcast_event(None, event, tx).unwrap() {
                Ok(_) => res,
                Err(WriteConflict) => todo!("See module_host_actor::call_reducer_with_tx"),
            }
        } else {
            db.finish_tx(tx, res)
        }
    }
}

/// Like [`execute_sql`], but for providing your own `tx`.
///
/// Returns None if you pass a mutable query with an immutable tx.
pub fn execute_sql_tx<'a>(
    db: &RelationalDB,
    tx: impl Into<TxMode<'a>>,
    sql: &str,
    ast: Vec<CrudExpr>,
    auth: AuthCtx,
) -> Result<Option<Vec<MemTable>>, DBError> {
    let mut tx = tx.into();

    if matches!(tx, TxMode::Tx(_)) && !CrudExpr::is_reads(&ast) {
        return Ok(None);
    }

    let mut updates = Vec::new(); // No subscription updates in this path, because it requires owning the tx.
    execute(&mut DbProgram::new(db, &mut tx, auth), ast, sql, &mut updates).map(Some)
}

pub struct SqlResult {
    pub rows: Vec<ProductValue>,
    /// These metrics will be reported via `report_tx_metrics`.
    /// They should not be reported separately to avoid double counting.
    pub metrics: ExecutionMetrics,
}

/// Run the `SQL` string using the `auth` credentials
pub fn run(
    db: &RelationalDB,
    sql_text: &str,
    auth: AuthCtx,
    subs: Option<&ModuleSubscriptions>,
    head: &mut Vec<(Box<str>, AlgebraicType)>,
) -> Result<SqlResult, DBError> {
    // We parse the sql statement in a mutable transation.
    // If it turns out to be a query, we downgrade the tx.
    let (tx, stmt) = db.with_auto_rollback(db.begin_mut_tx(IsolationLevel::Serializable, Workload::Sql), |tx| {
        compile_sql_stmt(sql_text, &SchemaViewer::new(tx, &auth), &auth)
    })?;

    let mut metrics = ExecutionMetrics::default();

    match stmt {
        Statement::Select(stmt) => {
            // Up to this point, the tx has been read-only,
            // and hence there are no deltas to process.
            let (tx_data, tx_metrics_mut, tx) = tx.commit_downgrade(Workload::Sql);

            // Release the tx on drop, so that we record metrics.
            let mut tx = scopeguard::guard(tx, |tx| {
                let (tx_metrics_downgrade, reducer) = db.release_tx(tx);
                db.report_tx_metricses(&reducer, Some(&tx_data), Some(&tx_metrics_mut), &tx_metrics_downgrade);
            });

            // Compute the header for the result set
            stmt.for_each_return_field(|col_name, col_type| {
                head.push((col_name.into(), col_type.clone()));
            });

            // Evaluate the query
            let rows = execute_select_stmt(stmt, &DeltaTx::from(&*tx), &mut metrics, |plan| {
                check_row_limit(
                    &[&plan],
                    db,
                    &tx,
                    |plan, tx| plan.plan_iter().map(|plan| estimate_rows_scanned(tx, plan)).sum(),
                    &auth,
                )?;
                Ok(plan)
            })?;

            // Update transaction metrics
            tx.metrics.merge(metrics);

            Ok(SqlResult {
                rows,
                metrics: tx.metrics,
            })
        }
        Statement::DML(stmt) => {
            // An extra layer of auth is required for DML
            if auth.caller != auth.owner {
                return Err(anyhow!("Only owners are authorized to run SQL DML statements").into());
            }

            // Evaluate the mutation
            let (mut tx, _) = db.with_auto_rollback(tx, |tx| execute_dml_stmt(stmt, tx, &mut metrics))?;

            // Update transaction metrics
            tx.metrics.merge(metrics);

            // Commit the tx if there are no deltas to process
            if subs.is_none() {
                let metrics = tx.metrics;
                return db.commit_tx(tx).map(|tx_opt| {
                    if let Some((tx_data, tx_metrics, reducer)) = tx_opt {
                        db.report(&reducer, &tx_metrics, Some(&tx_data));
                    }
                    SqlResult { rows: vec![], metrics }
                });
            }

            // Otherwise downgrade the tx and process the deltas.
            // Note, we get the delta by downgrading the tx.
            // Hence we just pass a default `DatabaseUpdate` here.
            // It will ultimately be replaced with the correct one.
            match subs
                .unwrap()
                .commit_and_broadcast_event(
                    None,
                    ModuleEvent {
                        timestamp: Timestamp::now(),
                        caller_identity: auth.caller,
                        caller_connection_id: None,
                        function_call: ModuleFunctionCall {
                            reducer: String::new(),
                            reducer_id: u32::MAX.into(),
                            args: ArgsTuple::default(),
                        },
                        status: EventStatus::Committed(DatabaseUpdate::default()),
                        energy_quanta_used: EnergyQuanta::ZERO,
                        host_execution_duration: Duration::ZERO,
                        request_id: None,
                        timer: None,
                    },
                    tx,
                )
                .unwrap()
            {
                Err(WriteConflict) => {
                    todo!("See module_host_actor::call_reducer_with_tx")
                }
                Ok(_) => Ok(SqlResult { rows: vec![], metrics }),
            }
        }
    }
}

/// Translates a `FieldName` to the field's name.
pub fn translate_col(tx: &Tx, field: FieldName) -> Option<Box<str>> {
    Some(
        tx.get_schema(field.table)?
            .get_column(field.col.idx())?
            .col_name
            .clone(),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::datastore::system_tables::{
        StRowLevelSecurityRow, StTableFields, ST_ROW_LEVEL_SECURITY_ID, ST_TABLE_ID, ST_TABLE_NAME,
    };
    use crate::db::relational_db::tests_utils::{begin_tx, insert, with_auto_commit, TestDB};
    use crate::vm::tests::create_table_with_rows;
    use itertools::Itertools;
    use pretty_assertions::assert_eq;
    use spacetimedb_lib::bsatn::ToBsatn;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::error::{ResultTest, TestError};
    use spacetimedb_lib::relation::Header;
    use spacetimedb_lib::{AlgebraicValue, Identity};
    use spacetimedb_primitives::{col_list, ColId, TableId};
    use spacetimedb_sats::{product, AlgebraicType, ArrayValue, ProductType};
    use spacetimedb_vm::eval::test_helpers::create_game_data;

    pub(crate) fn execute_for_testing(
        db: &RelationalDB,
        sql_text: &str,
        q: Vec<CrudExpr>,
    ) -> Result<Vec<MemTable>, DBError> {
        let (subs, _runtime) = ModuleSubscriptions::for_test_new_runtime(Arc::new(db.clone()));
        execute_sql(db, sql_text, q, AuthCtx::for_testing(), Some(&subs))
    }

    /// Short-cut for simplify test execution
    pub(crate) fn run_for_testing(db: &RelationalDB, sql_text: &str) -> Result<Vec<ProductValue>, DBError> {
        let (subs, _runtime) = ModuleSubscriptions::for_test_new_runtime(Arc::new(db.clone()));
        run(db, sql_text, AuthCtx::for_testing(), Some(&subs), &mut vec![]).map(|x| x.rows)
    }

    fn create_data(total_rows: u64) -> ResultTest<(TestDB, MemTable)> {
        let stdb = TestDB::durable()?;

        let rows: Vec<_> = (1..=total_rows)
            .map(|i| product!(i, format!("health{i}").into_boxed_str()))
            .collect();
        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let schema = with_auto_commit(&stdb, |tx| {
            create_table_with_rows(&stdb, tx, "inventory", head.clone(), &rows, StAccess::Public)
        })?;
        let header = Header::from(&*schema).into();

        Ok((stdb, MemTable::new(header, schema.table_access, rows)))
    }

    fn create_identity_table(table_name: &str) -> ResultTest<(TestDB, MemTable)> {
        let stdb = TestDB::durable()?;
        let head = ProductType::from([("identity", AlgebraicType::identity())]);
        let rows = vec![product!(Identity::ZERO), product!(Identity::ONE)];

        let schema = with_auto_commit(&stdb, |tx| {
            create_table_with_rows(&stdb, tx, table_name, head.clone(), &rows, StAccess::Public)
        })?;
        let header = Header::from(&*schema).into();

        Ok((stdb, MemTable::new(header, schema.table_access, rows)))
    }

    #[test]
    fn test_select_star() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        assert_eq!(result, input.data, "Inventory");
        Ok(())
    }

    #[test]
    fn test_limit() -> ResultTest<()> {
        let (db, _) = create_data(5)?;

        let result = run_for_testing(&db, "SELECT * FROM inventory limit 2")?;

        let (_, input) = create_data(2)?;

        assert_eq!(result, input.data, "Inventory");
        Ok(())
    }

    #[test]
    fn test_count() -> ResultTest<()> {
        let (db, _) = create_data(5)?;

        let sql = "SELECT count(*) as n FROM inventory";
        let result = run_for_testing(&db, sql)?;
        assert_eq!(result, vec![product![5u64]], "Inventory");

        let sql = "SELECT count(*) as n FROM inventory limit 2";
        let result = run_for_testing(&db, sql)?;
        assert_eq!(result, vec![product![5u64]], "Inventory");

        let sql = "SELECT count(*) as n FROM inventory WHERE inventory_id = 4 or inventory_id = 5";
        let result = run_for_testing(&db, sql)?;
        assert_eq!(result, vec![product![2u64]], "Inventory");
        Ok(())
    }

    /// Test the evaluation of SELECT, UPDATE, and DELETE parameterized with `:sender`
    #[test]
    fn test_sender_param() -> ResultTest<()> {
        let (db, _) = create_identity_table("user")?;

        const SELECT_ALL: &str = "SELECT * FROM user";

        let sql = "SELECT * FROM user WHERE identity = :sender";
        let result = run_for_testing(&db, sql)?;
        assert_eq!(result, vec![product![Identity::ZERO]]);

        let sql = "DELETE FROM user WHERE identity = :sender";
        run_for_testing(&db, sql)?;
        let result = run_for_testing(&db, SELECT_ALL)?;
        assert_eq!(result, vec![product![Identity::ONE]]);

        let zero = "0".repeat(64);
        let one = "0".repeat(63) + "1";

        let sql = format!("UPDATE user SET identity = 0x{zero}");
        run_for_testing(&db, &sql)?;
        let sql = format!("UPDATE user SET identity = 0x{one} WHERE identity = :sender");
        run_for_testing(&db, &sql)?;
        let result = run_for_testing(&db, SELECT_ALL)?;
        assert_eq!(result, vec![product![Identity::ONE]]);

        Ok(())
    }

    /// Create an [Identity] from a [u8]
    fn identity_from_u8(v: u8) -> Identity {
        Identity::from_byte_array([v; 32])
    }

    /// Insert rules into the RLS system table
    fn insert_rls_rules(
        db: &RelationalDB,
        table_ids: impl IntoIterator<Item = TableId>,
        rules: impl IntoIterator<Item = &'static str>,
    ) -> anyhow::Result<()> {
        with_auto_commit(db, |tx| {
            for (table_id, sql) in table_ids.into_iter().zip(rules) {
                db.insert(
                    tx,
                    ST_ROW_LEVEL_SECURITY_ID,
                    &ProductValue::from(StRowLevelSecurityRow {
                        table_id,
                        sql: sql.into(),
                    })
                    .to_bsatn_vec()?,
                )?;
            }
            Ok(())
        })
    }

    /// Insert product values into a table
    fn insert_rows(
        db: &RelationalDB,
        table_id: TableId,
        rows: impl IntoIterator<Item = ProductValue>,
    ) -> anyhow::Result<()> {
        with_auto_commit(db, |tx| {
            for row in rows.into_iter() {
                db.insert(tx, table_id, &row.to_bsatn_vec()?)?;
            }
            Ok(())
        })
    }

    /// Assert this query returns the expected rows for this user
    fn assert_query_results(
        db: &RelationalDB,
        sql: &str,
        auth: &AuthCtx,
        expected: impl IntoIterator<Item = ProductValue>,
    ) {
        assert_eq!(
            run(db, sql, *auth, None, &mut vec![])
                .unwrap()
                .rows
                .into_iter()
                .sorted()
                .dedup()
                .collect::<Vec<_>>(),
            expected.into_iter().sorted().dedup().collect::<Vec<_>>()
        );
    }

    /// Test a query that uses a multi-column index
    #[test]
    fn test_multi_column_index() -> anyhow::Result<()> {
        let db = TestDB::in_memory()?;

        let schema = [
            ("a", AlgebraicType::U64),
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
        ];

        let table_id = db.create_table_for_test_multi_column("t", &schema, [1, 2].into())?;

        insert_rows(
            &db,
            table_id,
            vec![
                product![0_u64, 1_u64, 2_u64],
                product![1_u64, 2_u64, 1_u64],
                product![2_u64, 2_u64, 2_u64],
            ],
        )?;

        assert_query_results(
            &db,
            "select * from t where c = 1 and b = 2",
            &AuthCtx::for_testing(),
            [product![1_u64, 2_u64, 1_u64]],
        );

        Ok(())
    }

    /// Test querying a table with RLS rules
    #[test]
    fn test_rls_rules() -> anyhow::Result<()> {
        let db = TestDB::in_memory()?;

        let id_for_a = identity_from_u8(1);
        let id_for_b = identity_from_u8(2);

        let users_schema = [("identity", AlgebraicType::identity())];
        let sales_schema = [
            ("order_id", AlgebraicType::U64),
            ("customer", AlgebraicType::identity()),
        ];

        let users_table_id = db.create_table_for_test("users", &users_schema, &[])?;
        let sales_table_id = db.create_table_for_test("sales", &sales_schema, &[])?;

        insert_rows(&db, users_table_id, vec![product![id_for_a], product![id_for_b]])?;
        insert_rows(
            &db,
            sales_table_id,
            vec![
                product![1u64, id_for_a],
                product![2u64, id_for_b],
                product![3u64, id_for_a],
                product![4u64, id_for_b],
            ],
        )?;

        insert_rls_rules(
            &db,
            [users_table_id, sales_table_id],
            [
                "select * from users where identity = :sender",
                "select s.* from users u join sales s on u.identity = s.customer",
            ],
        )?;

        let auth_for_a = AuthCtx::new(Identity::ZERO, id_for_a);
        let auth_for_b = AuthCtx::new(Identity::ZERO, id_for_b);

        assert_query_results(
            &db,
            // Should only return the identity for sender "a"
            "select * from users",
            &auth_for_a,
            [product![id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the identity for sender "b"
            "select * from users",
            &auth_for_b,
            [product![id_for_b]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            "select * from users where identity = :sender",
            &auth_for_a,
            [product![id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            "select * from users where identity = :sender",
            &auth_for_b,
            [product![id_for_b]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            &format!("select * from users where identity = 0x{}", id_for_a.to_hex()),
            &auth_for_a,
            [product![id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            &format!("select * from users where identity = 0x{}", id_for_b.to_hex()),
            &auth_for_b,
            [product![id_for_b]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            &format!(
                "select * from users where identity = :sender and identity = 0x{}",
                id_for_a.to_hex()
            ),
            &auth_for_a,
            [product![id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            &format!(
                "select * from users where identity = :sender and identity = 0x{}",
                id_for_b.to_hex()
            ),
            &auth_for_b,
            [product![id_for_b]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            &format!(
                "select * from users where identity = :sender or identity = 0x{}",
                id_for_b.to_hex()
            ),
            &auth_for_a,
            [product![id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            &format!(
                "select * from users where identity = :sender or identity = 0x{}",
                id_for_a.to_hex()
            ),
            &auth_for_b,
            [product![id_for_b]],
        );
        assert_query_results(
            &db,
            // Should not return any rows.
            // Querying as sender "a", but filtering on sender "b".
            &format!("select * from users where identity = 0x{}", id_for_b.to_hex()),
            &auth_for_a,
            [],
        );
        assert_query_results(
            &db,
            // Should not return any rows.
            // Querying as sender "b", but filtering on sender "a".
            &format!("select * from users where identity = 0x{}", id_for_a.to_hex()),
            &auth_for_b,
            [],
        );
        assert_query_results(
            &db,
            // Should not return any rows.
            // Querying as sender "a", but filtering on sender "b".
            &format!(
                "select * from users where identity = :sender and identity = 0x{}",
                id_for_b.to_hex()
            ),
            &auth_for_a,
            [],
        );
        assert_query_results(
            &db,
            // Should not return any rows.
            // Querying as sender "b", but filtering on sender "a".
            &format!(
                "select * from users where identity = :sender and identity = 0x{}",
                id_for_a.to_hex()
            ),
            &auth_for_b,
            [],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            "select * from sales",
            &auth_for_a,
            [product![1u64, id_for_a], product![3u64, id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            "select * from sales",
            &auth_for_b,
            [product![2u64, id_for_b], product![4u64, id_for_b]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            "select s.* from users u join sales s on u.identity = s.customer",
            &auth_for_a,
            [product![1u64, id_for_a], product![3u64, id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            "select s.* from users u join sales s on u.identity = s.customer",
            &auth_for_b,
            [product![2u64, id_for_b], product![4u64, id_for_b]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "a"
            "select s.* from users u join sales s on u.identity = s.customer where u.identity = :sender",
            &auth_for_a,
            [product![1u64, id_for_a], product![3u64, id_for_a]],
        );
        assert_query_results(
            &db,
            // Should only return the orders for sender "b"
            "select s.* from users u join sales s on u.identity = s.customer where u.identity = :sender",
            &auth_for_b,
            [product![2u64, id_for_b], product![4u64, id_for_b]],
        );

        Ok(())
    }

    /// Test querying tables with multiple levels of RLS rules
    #[test]
    fn test_nested_rls_rules() -> anyhow::Result<()> {
        let db = TestDB::in_memory()?;

        let id_for_a = identity_from_u8(1);
        let id_for_b = identity_from_u8(2);
        let id_for_c = identity_from_u8(3);

        let users_schema = [("identity", AlgebraicType::identity())];
        let sales_schema = [
            ("order_id", AlgebraicType::U64),
            ("product_id", AlgebraicType::U64),
            ("customer", AlgebraicType::identity()),
        ];

        let users_table_id = db.create_table_for_test("users", &users_schema, &[0.into()])?;
        let admin_table_id = db.create_table_for_test("admins", &users_schema, &[0.into()])?;
        let sales_table_id = db.create_table_for_test("sales", &sales_schema, &[0.into()])?;

        insert_rows(&db, admin_table_id, [product![id_for_c]])?;
        insert_rows(
            &db,
            users_table_id,
            [product![id_for_a], product![id_for_b], product![id_for_c]],
        )?;
        insert_rows(
            &db,
            sales_table_id,
            [product![1u64, 1u64, id_for_a], product![2u64, 2u64, id_for_b]],
        )?;

        insert_rls_rules(
            &db,
            [admin_table_id, users_table_id, users_table_id, sales_table_id],
            [
                "select * from admins where identity = :sender",
                "select * from users where identity = :sender",
                "select users.* from admins join users",
                "select s.* from users u join sales s on u.identity = s.customer",
            ],
        )?;

        let auth_for_a = AuthCtx::new(Identity::ZERO, id_for_a);
        let auth_for_b = AuthCtx::new(Identity::ZERO, id_for_b);
        let auth_for_c = AuthCtx::new(Identity::ZERO, id_for_c);

        assert_query_results(
            &db,
            "select * from admins",
            &auth_for_a,
            // Identity "a" is not an admin
            [],
        );
        assert_query_results(
            &db,
            "select * from admins",
            &auth_for_b,
            // Identity "b" is not an admin
            [],
        );
        assert_query_results(
            &db,
            "select * from admins",
            &auth_for_c,
            // Identity "c" is an admin
            [product![id_for_c]],
        );

        assert_query_results(
            &db,
            "select * from users",
            &auth_for_a,
            // Identity "a" can only see its own user
            vec![product![id_for_a]],
        );
        assert_query_results(
            &db,
            "select * from users",
            &auth_for_b,
            // Identity "b" can only see its own user
            vec![product![id_for_b]],
        );
        assert_query_results(
            &db,
            "select * from users",
            &auth_for_c,
            // Identity "c" is an admin so it can see everyone's users
            [product![id_for_a], product![id_for_b], product![id_for_c]],
        );

        assert_query_results(
            &db,
            "select * from sales",
            &auth_for_a,
            // Identity "a" can only see its own orders
            [product![1u64, 1u64, id_for_a]],
        );
        assert_query_results(
            &db,
            "select * from sales",
            &auth_for_b,
            // Identity "b" can only see its own orders
            [product![2u64, 2u64, id_for_b]],
        );
        assert_query_results(
            &db,
            "select * from sales",
            &auth_for_c,
            // Identity "c" is an admin so it can see everyone's orders
            [product![1u64, 1u64, id_for_a], product![2u64, 2u64, id_for_b]],
        );

        Ok(())
    }

    /// Test projecting columns from both tables in join
    #[test]
    fn test_project_join() -> anyhow::Result<()> {
        let db = TestDB::in_memory()?;

        let t_schema = [("id", AlgebraicType::U8), ("x", AlgebraicType::U8)];
        let s_schema = [("id", AlgebraicType::U8), ("y", AlgebraicType::U8)];

        let t_id = db.create_table_for_test("t", &t_schema, &[0.into()])?;
        let s_id = db.create_table_for_test("s", &s_schema, &[0.into()])?;

        insert_rows(&db, t_id, [product![1_u8, 2_u8]])?;
        insert_rows(&db, s_id, [product![1_u8, 3_u8]])?;

        let id = identity_from_u8(1);
        let auth = AuthCtx::new(Identity::ZERO, id);

        assert_query_results(
            &db,
            "select t.x, s.y from t join s on t.id = s.id",
            &auth,
            [product![2_u8, 3_u8]],
        );

        Ok(())
    }

    #[test]
    fn test_select_star_table() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory.* FROM inventory")?;

        assert_eq!(result, input.data, "Inventory");

        let result = run_for_testing(
            &db,
            "SELECT inventory.inventory_id FROM inventory WHERE inventory.inventory_id = 1",
        )?;

        assert_eq!(result, vec![product!(1u64)], "Inventory");

        Ok(())
    }

    #[test]
    fn test_select_catalog() -> ResultTest<()> {
        let (db, _) = create_data(1)?;

        let tx = begin_tx(&db);
        let _ = db.release_tx(tx);

        let result = run_for_testing(
            &db,
            &format!("SELECT * FROM {} WHERE table_id = {}", ST_TABLE_NAME, ST_TABLE_ID),
        )?;

        let pk_col_id: ColId = StTableFields::TableId.into();
        let row = product![
            ST_TABLE_ID,
            ST_TABLE_NAME,
            StTableType::System.as_str(),
            StAccess::Public.as_str(),
            Some(AlgebraicValue::Array(ArrayValue::U16(vec![pk_col_id.0].into()))),
        ];

        assert_eq!(result, vec![row], "st_table");
        Ok(())
    }

    #[test]
    fn test_select_column() -> ResultTest<()> {
        let (db, _) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory_id FROM inventory")?;

        let row = product![1u64];

        assert_eq!(result, vec![row], "Inventory");
        Ok(())
    }

    #[test]
    fn test_where() -> ResultTest<()> {
        let (db, _) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory_id FROM inventory WHERE inventory_id = 1")?;

        let row = product![1u64];

        assert_eq!(result, vec![row], "Inventory");
        Ok(())
    }

    #[test]
    fn test_or() -> ResultTest<()> {
        let (db, _) = create_data(2)?;

        let mut result = run_for_testing(
            &db,
            "SELECT inventory_id FROM inventory WHERE inventory_id = 1 OR inventory_id = 2",
        )?;

        result.sort();

        assert_eq!(result, vec![product![1u64], product![2u64]], "Inventory");
        Ok(())
    }

    #[test]
    fn test_nested() -> ResultTest<()> {
        let (db, _) = create_data(2)?;

        let mut result = run_for_testing(
            &db,
            "SELECT inventory_id FROM inventory WHERE (inventory_id = 1 OR inventory_id = 2 AND (true))",
        )?;

        result.sort();

        assert_eq!(result, vec![product![1u64], product![2u64]], "Inventory");
        Ok(())
    }

    #[test]
    fn test_inner_join() -> ResultTest<()> {
        let data = create_game_data();

        let db = TestDB::durable()?;

        with_auto_commit::<_, TestError>(&db, |tx| {
            let i = create_table_with_rows(&db, tx, "Inventory", data.inv_ty, &data.inv.data, StAccess::Public)?;
            let p = create_table_with_rows(&db, tx, "Player", data.player_ty, &data.player.data, StAccess::Public)?;
            create_table_with_rows(
                &db,
                tx,
                "Location",
                data.location_ty,
                &data.location.data,
                StAccess::Public,
            )?;
            Ok((p, i))
        })?;

        let result = run_for_testing(
            &db,
            "SELECT
        Player.*
            FROM
        Player
        JOIN Location
        ON Location.entity_id = Player.entity_id
        WHERE Location.x > 0 AND Location.x <= 32 AND Location.z > 0 AND Location.z <= 32",
        )?;

        let row1 = product!(100u64, 1u64);

        assert_eq!(result, vec![row1], "Player JOIN Location");

        let result = run_for_testing(
            &db,
            "SELECT
        Inventory.*
            FROM
        Inventory
        JOIN Player
        ON Inventory.inventory_id = Player.inventory_id
        JOIN Location
        ON Player.entity_id = Location.entity_id
        WHERE Location.x > 0 AND Location.x <= 32 AND Location.z > 0 AND Location.z <= 32",
        )?;

        let row1 = product!(1u64, "health");

        assert_eq!(result, vec![row1], "Inventory JOIN Player JOIN Location");
        Ok(())
    }

    #[test]
    fn test_insert() -> ResultTest<()> {
        let (db, mut input) = create_data(1)?;

        let result = run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 'test')")?;

        assert_eq!(result.len(), 0, "Return results");

        let mut result = run_for_testing(&db, "SELECT * FROM inventory")?;

        input.data.push(product![2u64, "test"]);
        input.data.sort();
        result.sort();

        assert_eq!(result, input.data, "Inventory");

        Ok(())
    }

    #[test]
    fn test_delete() -> ResultTest<()> {
        let (db, _input) = create_data(1)?;

        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 't2')")?;
        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (3, 't3')")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;
        assert_eq!(result.len(), 3, "Not return results");

        run_for_testing(&db, "DELETE FROM inventory WHERE inventory.inventory_id = 3")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;
        assert_eq!(result.len(), 2, "Not delete correct row?");

        run_for_testing(&db, "DELETE FROM inventory")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;
        assert_eq!(result.len(), 0, "Not delete all rows");

        Ok(())
    }

    #[test]
    fn test_update() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 't2')")?;
        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (3, 't3')")?;

        run_for_testing(&db, "UPDATE inventory SET name = 'c2' WHERE inventory_id = 2")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory WHERE inventory_id = 2")?;

        let mut change = input;
        change.data.clear();
        change.data.push(product![2u64, "c2"]);

        assert_eq!(result, change.data, "Update Inventory 2");

        run_for_testing(&db, "UPDATE inventory SET name = 'c3'")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        let updated: Vec<_> = result
            .into_iter()
            .map(|x| x.field_as_str(1, None).unwrap().to_string())
            .collect();
        assert_eq!(vec!["c3"; 3], updated);

        Ok(())
    }

    #[test]
    fn test_multi_column() -> ResultTest<()> {
        let (db, _input) = create_data(1)?;

        // Create table [test] with index on [a, b]
        let schema = &[
            ("a", AlgebraicType::I32),
            ("b", AlgebraicType::I32),
            ("c", AlgebraicType::I32),
            ("d", AlgebraicType::I32),
        ];
        let table_id = db.create_table_for_test_multi_column("test", schema, col_list![0, 1])?;
        with_auto_commit(&db, |tx| insert(&db, tx, table_id, &product![1, 1, 1, 1]).map(drop))?;

        let result = run_for_testing(&db, "select * from test where b = 1 and a = 1")?;

        assert_eq!(result, vec![product![1, 1, 1, 1]]);

        Ok(())
    }

    #[test]
    fn test_large_query_no_panic() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let _table_id = db
            .create_table_for_test_multi_column(
                "test",
                &[("x", AlgebraicType::I32), ("y", AlgebraicType::I32)],
                col_list![0, 1],
            )
            .unwrap();

        let mut query = "select * from test where ".to_string();
        for x in 0..1_000 {
            for y in 0..1_000 {
                let fragment = format!("((x = {x}) and y = {y}) or");
                query.push_str(&fragment);
            }
        }
        query.push_str("((x = 1000) and (y = 1000))");

        assert!(run_for_testing(&db, &query).is_err());
        Ok(())
    }

    #[test]
    fn test_impossible_bounds_no_panic() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = db
            .create_table_for_test("test", &[("x", AlgebraicType::I32)], &[ColId(0)])
            .unwrap();

        with_auto_commit(&db, |tx| {
            for i in 0..1000i32 {
                insert(&db, tx, table_id, &product!(i)).unwrap();
            }
            Ok::<(), DBError>(())
        })
        .unwrap();

        let result = run_for_testing(&db, "select * from test where x > 5 and x < 5").unwrap();
        assert!(result.is_empty());

        let result = run_for_testing(&db, "select * from test where x >= 5 and x < 4").unwrap();
        assert!(result.is_empty(), "Expected no rows but found {:#?}", result);

        let result = run_for_testing(&db, "select * from test where x > 5 and x <= 4").unwrap();
        assert!(result.is_empty());
        Ok(())
    }

    #[test]
    fn test_multi_column_two_ranges() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] with index on [a, b]
        let schema = &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)];
        let table_id = db.create_table_for_test_multi_column("test", schema, col_list![0, 1])?;
        let row = product![4u8, 8u8];
        with_auto_commit(&db, |tx| insert(&db, tx, table_id, &row.clone()).map(drop))?;

        let result = run_for_testing(&db, "select * from test where a >= 3 and a <= 5 and b >= 3 and b <= 5")?;

        assert!(result.is_empty());

        Ok(())
    }

    #[test]
    fn test_row_limit() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        with_auto_commit(&db, |tx| -> Result<_, DBError> {
            for i in 0..5u8 {
                insert(&db, tx, table_id, &product!(i))?;
            }
            Ok(())
        })?;

        let server = Identity::from_claims("issuer", "server");
        let client = Identity::from_claims("issuer", "client");

        let internal_auth = AuthCtx::new(server, server);
        let external_auth = AuthCtx::new(server, client);

        let run = |db, sql, auth, subs| run(db, sql, auth, subs, &mut vec![]);

        // No row limit, both queries pass.
        assert!(run(&db, "SELECT * FROM T", internal_auth, None).is_ok());
        assert!(run(&db, "SELECT * FROM T", external_auth, None).is_ok());

        // Set row limit.
        assert!(run(&db, "SET row_limit = 4", internal_auth, None).is_ok());

        // External query fails.
        assert!(run(&db, "SELECT * FROM T", internal_auth, None).is_ok());
        assert!(run(&db, "SELECT * FROM T", external_auth, None).is_err());

        // Increase row limit.
        assert!(run(&db, "DELETE FROM st_var WHERE name = 'row_limit'", internal_auth, None).is_ok());
        assert!(run(&db, "SET row_limit = 5", internal_auth, None).is_ok());

        // Both queries pass.
        assert!(run(&db, "SELECT * FROM T", internal_auth, None).is_ok());
        assert!(run(&db, "SELECT * FROM T", external_auth, None).is_ok());

        Ok(())
    }

    // Verify we don't return rows on DML
    #[test]
    fn test_row_dml() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        with_auto_commit(&db, |tx| -> Result<_, DBError> {
            for i in 0..4u8 {
                insert(&db, tx, table_id, &product!(i))?;
            }
            Ok(())
        })?;

        let server = Identity::from_claims("issuer", "server");

        let internal_auth = AuthCtx::new(server, server);

        let run = |db, sql, auth, subs| run(db, sql, auth, subs, &mut vec![]);

        let check = |db, sql, auth, metrics: ExecutionMetrics| {
            let result = run(db, sql, auth, None)?;
            assert_eq!(result.rows, vec![]);
            assert_eq!(result.metrics.rows_inserted, metrics.rows_inserted);
            assert_eq!(result.metrics.rows_deleted, metrics.rows_deleted);
            assert_eq!(result.metrics.rows_updated, metrics.rows_updated);

            Ok::<(), DBError>(())
        };

        let ins = ExecutionMetrics {
            rows_inserted: 1,
            ..ExecutionMetrics::default()
        };
        let upd = ExecutionMetrics {
            rows_updated: 5,
            ..ExecutionMetrics::default()
        };
        let del = ExecutionMetrics {
            rows_deleted: 1,
            ..ExecutionMetrics::default()
        };

        check(&db, "INSERT INTO T (a) VALUES (5)", internal_auth, ins)?;
        check(&db, "UPDATE T SET a = 2", internal_auth, upd)?;
        assert_eq!(
            run(&db, "SELECT * FROM T", internal_auth, None)?.rows,
            vec![product!(2u8)]
        );
        check(&db, "DELETE FROM T", internal_auth, del)?;

        Ok(())
    }
}
