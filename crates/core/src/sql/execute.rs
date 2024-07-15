use std::time::Duration;

use super::compiler::compile_sql;
use crate::db::datastore::locking_tx_datastore::state_view::StateView;
use crate::db::datastore::system_tables::StVarTable;
use crate::db::datastore::traits::IsolationLevel;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::ArgsTuple;
use crate::subscription::module_subscription_actor::{ModuleSubscriptions, WriteConflict};
use crate::util::slow::SlowQueryLogger;
use crate::vm::{DbProgram, TxMode};
use itertools::Either;
use spacetimedb_client_api_messages::timestamp::Timestamp;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::FieldName;
use spacetimedb_lib::{ProductType, ProductValue};
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

pub fn ctx_sql(db: &RelationalDB) -> ExecutionContext {
    ExecutionContext::sql(db.address())
}

fn execute(
    p: &mut DbProgram<'_, '_>,
    ast: Vec<CrudExpr>,
    sql: &str,
    updates: &mut Vec<DatabaseTableUpdate>,
) -> Result<Vec<MemTable>, DBError> {
    let slow_query_threshold = if let TxMode::Tx(tx) = p.tx {
        StVarTable::query_limit(p.ctx, p.db, tx)?.map(Duration::from_millis)
    } else {
        None
    };
    let _slow_query_logger = SlowQueryLogger::new(sql, slow_query_threshold, p.ctx.workload()).log_guard();
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
    let ctx = ctx_sql(db);
    if CrudExpr::is_reads(&ast) {
        let mut updates = Vec::new();
        db.with_read_only(&ctx, |tx| {
            execute(
                &mut DbProgram::new(&ctx, db, &mut TxMode::Tx(tx), auth),
                ast,
                sql,
                &mut updates,
            )
        })
    } else if subs.is_none() {
        let mut updates = Vec::new();
        db.with_auto_commit(&ctx, |mut_tx| {
            execute(
                &mut DbProgram::new(&ctx, db, &mut mut_tx.into(), auth),
                ast,
                sql,
                &mut updates,
            )
        })
    } else {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let mut updates = Vec::with_capacity(ast.len());
        let res = execute(
            &mut DbProgram::new(&ctx, db, &mut (&mut tx).into(), auth),
            ast,
            sql,
            &mut updates,
        );
        if res.is_ok() && !updates.is_empty() {
            let event = ModuleEvent {
                timestamp: Timestamp::now(),
                caller_identity: auth.caller,
                caller_address: None,
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
            match subs.unwrap().commit_and_broadcast_event(None, event, &ctx, tx).unwrap() {
                Ok(_) => res,
                Err(WriteConflict) => todo!("See module_host_actor::call_reducer_with_tx"),
            }
        } else {
            db.finish_tx(&ctx, tx, res)
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

    let ctx = ctx_sql(db);
    let mut updates = Vec::new(); // No subscription updates in this path, because it requires owning the tx.
    execute(&mut DbProgram::new(&ctx, db, &mut tx, auth), ast, sql, &mut updates).map(Some)
}

/// Run the `SQL` string using the `auth` credentials
pub fn run(
    db: &RelationalDB,
    sql_text: &str,
    auth: AuthCtx,
    subs: Option<&ModuleSubscriptions>,
) -> Result<Vec<MemTable>, DBError> {
    let ctx = ctx_sql(db);
    let result = db.with_read_only(&ctx, |tx| {
        let ast = compile_sql(db, tx, sql_text)?;
        if CrudExpr::is_reads(&ast) {
            let mut updates = Vec::new();
            let result = execute(
                &mut DbProgram::new(&ctx, db, &mut TxMode::Tx(tx), auth),
                ast,
                sql_text,
                &mut updates,
            )?;
            Ok::<_, DBError>(Either::Left(result))
        } else {
            // hehe. right. write.
            Ok(Either::Right(ast))
        }
    })?;
    match result {
        Either::Left(result) => Ok(result),
        // TODO: this should perhaps be an upgradable_read upgrade? or we should try
        //       and figure out if we can detect the mutablility of the query before we take
        //       the tx? once we have migrations we probably don't want to have stale
        //       sql queries after a database schema have been updated.
        Either::Right(ast) => execute_sql(db, sql_text, ast, auth, subs),
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
    use super::*;
    use crate::db::datastore::system_tables::{ST_TABLES_ID, ST_TABLES_NAME};
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::vm::tests::create_table_with_rows;
    use spacetimedb_lib::error::{ResultTest, TestError};
    use spacetimedb_lib::relation::ColExpr;
    use spacetimedb_lib::Identity;
    use spacetimedb_primitives::{col_list, ColId};
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::relation::Header;
    use spacetimedb_sats::{product, AlgebraicType, ProductType};
    use spacetimedb_vm::eval::test_helpers::{create_game_data, mem_table, mem_table_without_table_name};
    use std::sync::Arc;

    pub(crate) fn execute_for_testing(
        db: &RelationalDB,
        sql_text: &str,
        q: Vec<CrudExpr>,
    ) -> Result<Vec<MemTable>, DBError> {
        let subs = ModuleSubscriptions::new(Arc::new(db.clone()), Identity::ZERO);
        execute_sql(db, sql_text, q, AuthCtx::for_testing(), Some(&subs))
    }

    /// Short-cut for simplify test execution
    pub(crate) fn run_for_testing(db: &RelationalDB, sql_text: &str) -> Result<Vec<MemTable>, DBError> {
        let subs = ModuleSubscriptions::new(Arc::new(db.clone()), Identity::ZERO);
        run(db, sql_text, AuthCtx::for_testing(), Some(&subs))
    }

    fn create_data(total_rows: u64) -> ResultTest<(TestDB, MemTable)> {
        let stdb = TestDB::durable()?;

        let rows: Vec<_> = (1..=total_rows)
            .map(|i| product!(i, format!("health{i}").into_boxed_str()))
            .collect();
        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let schema = stdb.with_auto_commit(&ExecutionContext::default(), |tx| {
            create_table_with_rows(&stdb, tx, "inventory", head.clone(), &rows, StAccess::Public)
        })?;
        let header = Header::from(&*schema).into();

        Ok((stdb, MemTable::new(header, schema.table_access, rows)))
    }

    #[test]
    fn test_select_star() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_select_star_table() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory.* FROM inventory")?;
        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );

        let result = run_for_testing(
            &db,
            "SELECT inventory.inventory_id FROM inventory WHERE inventory.inventory_id = 1",
        )?;
        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        let head = ProductType::from([("inventory_id", AlgebraicType::U64)]);
        let row = product!(1u64);
        let input = mem_table(input.head.table_id, head, vec![row]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );

        Ok(())
    }

    #[test]
    fn test_select_scalar() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT 1 FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();
        let schema = ProductType::from([AlgebraicType::I32]);
        let row = product!(1);
        let input = mem_table(input.head.table_id, schema, vec![row]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Scalar"
        );
        Ok(())
    }

    #[test]
    fn test_select_multiple() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT * FROM inventory;\nSELECT * FROM inventory")?;

        assert_eq!(result.len(), 2, "Not return results");

        for result in result {
            assert_eq!(
                mem_table_without_table_name(&result),
                mem_table_without_table_name(&input),
                "Inventory"
            );
        }
        Ok(())
    }

    #[test]
    fn test_select_catalog() -> ResultTest<()> {
        let (db, _) = create_data(1)?;

        let tx = db.begin_tx();
        let schema = db.schema_for_table(&tx, ST_TABLES_ID).unwrap();
        db.release_tx(&ExecutionContext::internal(db.address()), tx);
        let result = run_for_testing(
            &db,
            &format!("SELECT * FROM {} WHERE table_id = {}", ST_TABLES_NAME, ST_TABLES_ID),
        )?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();
        let row = product![
            ST_TABLES_ID,
            ST_TABLES_NAME,
            StTableType::System.as_str(),
            StAccess::Public.as_str(),
        ];
        let input = MemTable::new(Header::from(&*schema).into(), schema.table_access, vec![row]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "st_table"
        );
        Ok(())
    }

    #[test]
    fn test_select_column() -> ResultTest<()> {
        let (db, table) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory_id FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();
        // The expected result.
        let inv = table.head.project(&[ColExpr::Col(0.into())]).unwrap();

        let row = product![1u64];
        let input = MemTable::new(inv.into(), table.table_access, vec![row]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_where() -> ResultTest<()> {
        let (db, table) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory_id FROM inventory WHERE inventory_id = 1")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        // The expected result.
        let inv = table.head.project(&[ColExpr::Col(0.into())]).unwrap();

        let row = product![1u64];
        let input = MemTable::new(inv.into(), table.table_access, vec![row]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_or() -> ResultTest<()> {
        let (db, table) = create_data(2)?;

        let result = run_for_testing(
            &db,
            "SELECT inventory_id FROM inventory WHERE inventory_id = 1 OR inventory_id = 2",
        )?;

        assert_eq!(result.len(), 1, "Not return results");
        let mut result = result.first().unwrap().clone();
        result.data.sort();
        //The expected result
        let inv = table.head.project(&[ColExpr::Col(0.into())]).unwrap();

        let input = MemTable::new(inv.into(), table.table_access, vec![product![1u64], product![2u64]]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_nested() -> ResultTest<()> {
        let (db, table) = create_data(2)?;

        let result = run_for_testing(
            &db,
            "SELECT (inventory_id) FROM inventory WHERE (inventory_id = 1 OR inventory_id = 2 AND (1=1))",
        )?;

        assert_eq!(result.len(), 1, "Not return results");
        let mut result = result.first().unwrap().clone();
        result.data.sort();
        // The expected result.
        let inv = table.head.project(&[ColExpr::Col(0.into())]).unwrap();

        let input = MemTable::new(inv.into(), table.table_access, vec![product![1u64], product![2u64]]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_inner_join() -> ResultTest<()> {
        let data = create_game_data();

        let db = TestDB::durable()?;

        let (p_schema, inv_schema) = db.with_auto_commit::<_, _, TestError>(&ExecutionContext::default(), |tx| {
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

        let result = &run_for_testing(
            &db,
            "SELECT
        Player.*
            FROM
        Player
        JOIN Location
        ON Location.entity_id = Player.entity_id
        WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32",
        )?[0];

        let row1 = product!(100u64, 1u64);
        let input = MemTable::new(Header::from(&*p_schema).into(), p_schema.table_access, [row1].into());

        assert_eq!(
            mem_table_without_table_name(result),
            mem_table_without_table_name(&input),
            "Player JOIN Location"
        );

        let result = &run_for_testing(
            &db,
            "SELECT
        Inventory.*
            FROM
        Inventory
        JOIN Player
        ON Inventory.inventory_id = Player.inventory_id
        JOIN Location
        ON Player.entity_id = Location.entity_id
        WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32",
        )?[0];

        let row1 = product!(1u64, "health");
        let input = MemTable::new(
            Header::from(&*inv_schema).into(),
            inv_schema.table_access,
            [row1].into(),
        );

        assert_eq!(
            mem_table_without_table_name(result),
            mem_table_without_table_name(&input),
            "Inventory JOIN Player JOIN Location"
        );
        Ok(())
    }

    #[test]
    fn test_insert() -> ResultTest<()> {
        let (db, mut input) = create_data(1)?;

        let result = run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 'test')")?;

        assert_eq!(result.len(), 0, "Return results");

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let mut result = result.first().unwrap().clone();

        input.data.push(product![2u64, "test"]);
        input.data.sort();
        result.data.sort();

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );

        Ok(())
    }

    #[test]
    fn test_delete() -> ResultTest<()> {
        let (db, _input) = create_data(1)?;

        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 't2')")?;
        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (3, 't3')")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;
        assert_eq!(
            result.iter().map(|x| x.data.len()).sum::<usize>(),
            3,
            "Not return results"
        );

        run_for_testing(&db, "DELETE FROM inventory WHERE inventory.inventory_id = 3")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;
        assert_eq!(
            result.iter().map(|x| x.data.len()).sum::<usize>(),
            2,
            "Not delete correct row?"
        );

        run_for_testing(&db, "DELETE FROM inventory")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;
        assert_eq!(
            result.iter().map(|x| x.data.len()).sum::<usize>(),
            0,
            "Not delete all rows"
        );

        Ok(())
    }

    #[test]
    fn test_update() -> ResultTest<()> {
        let (db, input) = create_data(1)?;

        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 't2')")?;
        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (3, 't3')")?;

        run_for_testing(&db, "UPDATE inventory SET name = 'c2' WHERE inventory_id = 2")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory WHERE inventory_id = 2")?;

        let result = result.first().unwrap().clone();

        let mut change = input;
        change.data.clear();
        change.data.push(product![2u64, "c2"]);

        assert_eq!(
            mem_table_without_table_name(&change),
            mem_table_without_table_name(&result),
            "Update Inventory 2"
        );

        run_for_testing(&db, "UPDATE inventory SET name = 'c3'")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        let updated: Vec<_> = result
            .into_iter()
            .map(|x| {
                x.data
                    .into_iter()
                    .map(|x| x.field_as_str(1, None).unwrap().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(vec![vec!["c3"; 3]], updated);

        Ok(())
    }

    fn cols_no_table_ids(head: &Header) -> Vec<(ColId, &AlgebraicType)> {
        head.fields
            .iter()
            .map(|x| (x.field.col, &x.algebraic_type))
            .collect::<Vec<_>>()
    }

    #[test]
    fn test_create_table() -> ResultTest<()> {
        let (db, _) = create_data(1)?;

        run_for_testing(&db, "CREATE TABLE inventory2 (inventory_id BIGINT UNSIGNED, name TEXT)")?;
        run_for_testing(
            &db,
            "INSERT INTO inventory2 (inventory_id, name) VALUES (1, 'health1') ",
        )?;

        let a = run_for_testing(&db, "SELECT * FROM inventory")?.swap_remove(0);
        let b = run_for_testing(&db, "SELECT * FROM inventory2")?.swap_remove(0);

        assert_eq!(a.data, b.data);
        assert_eq!(cols_no_table_ids(&a.head), cols_no_table_ids(&b.head));

        Ok(())
    }

    #[test]
    fn test_drop_table() -> ResultTest<()> {
        let (db, _) = create_data(1)?;

        run_for_testing(&db, "CREATE TABLE inventory2 (inventory_id BIGINT UNSIGNED, name TEXT)")?;

        run_for_testing(&db, "DROP TABLE inventory2")?;
        match run_for_testing(&db, "SELECT * FROM inventory2") {
            Ok(_) => {
                panic!("Fail to drop table");
            }
            Err(err) => {
                let msg = err.to_string();
                assert_eq!(
                    "SqlError: Unknown table: `inventory2`, executing: `SELECT * FROM inventory2`",
                    msg
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_column_constraints() -> ResultTest<()> {
        let (db, _) = create_data(0)?;

        fn check_column(
            db: &RelationalDB,
            table_name: &str,
            is_null: bool,
            is_autoinc: bool,
            idx_uniq: Option<bool>,
        ) -> ResultTest<()> {
            let tx = db.begin_tx();
            let t = db.table_id_from_name(&tx, table_name)?.unwrap();
            let t = db.schema_for_table(&tx, t)?;

            let col = t.columns().first().unwrap();
            let idx = t.indexes.first().map(|x| x.is_unique);
            let column_auto_inc = t
                .constraints
                .first()
                .map(|x| x.constraints.has_autoinc())
                .unwrap_or(false);
            let column_auto_inc =
                column_auto_inc || t.sequences.first().map(|x| x.col_pos == col.col_pos).unwrap_or(false);

            if is_null {
                assert_eq!(
                    col.col_type,
                    AlgebraicType::option(AlgebraicType::I64),
                    "Null type {}.{}",
                    table_name,
                    col.col_name
                )
            }
            assert_eq!(
                column_auto_inc, is_autoinc,
                "is_autoinc {}.{}",
                table_name, col.col_name
            );
            assert_eq!(idx, idx_uniq, "idx_uniq {}.{}", table_name, col.col_name);

            Ok(())
        }

        run_for_testing(&db, "CREATE TABLE a (inventory_id BIGINT NULL)")?;
        check_column(&db, "a", true, false, None)?;

        run_for_testing(&db, "CREATE TABLE b (inventory_id BIGINT NOT NULL)")?;
        check_column(&db, "b", false, false, None)?;

        run_for_testing(&db, "CREATE TABLE c (inventory_id BIGINT UNIQUE)")?;
        check_column(&db, "c", false, false, Some(true))?;

        run_for_testing(&db, "CREATE TABLE d (inventory_id BIGINT PRIMARY KEY)")?;
        check_column(&db, "d", false, false, Some(true))?;

        run_for_testing(
            &db,
            "CREATE TABLE e (inventory_id BIGINT GENERATED BY DEFAULT AS IDENTITY)",
        )?;
        check_column(&db, "e", false, true, Some(true))?;

        run_for_testing(
            &db,
            "CREATE TABLE f (inventory_id BIGINT PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY)",
        )?;
        check_column(&db, "f", false, true, Some(true))?;

        Ok(())
    }

    #[test]
    fn test_big_sql() -> ResultTest<()> {
        let (db, _input) = create_data(1)?;

        let result = run_for_testing(
            &db,
            "insert into inventory (inventory_id, name) values (1, 'Kiley');
insert into inventory (inventory_id, name) values (2, 'Terza');
insert into inventory (inventory_id, name) values (3, 'Alvie');
SELECT * FROM inventory",
        )?;

        let result = result.first().unwrap().clone();
        assert_eq!(result.data.len(), 4);

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
        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            db.insert(tx, table_id, product![1, 1, 1, 1])
        })?;

        let result = run_for_testing(&db, "select * from test where b = 1 and a = 1")?;

        let result = result.first().unwrap().clone();
        assert_eq!(result.data, vec![product![1, 1, 1, 1]]);

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
            .create_table_for_test("test", &[("x", AlgebraicType::I32)], &[(ColId(0), "test_x")])
            .unwrap();

        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            for i in 0..1000i32 {
                db.insert(tx, table_id, product!(i)).unwrap();
            }
            Ok::<(), DBError>(())
        })
        .unwrap();

        let result = run_for_testing(&db, "select * from test where x > 5 and x < 5").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].data.is_empty());

        let result = run_for_testing(&db, "select * from test where x >= 5 and x < 4").unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].data.is_empty(),
            "Expected no rows but found {:#?}",
            result[0].data
        );

        let result = run_for_testing(&db, "select * from test where x > 5 and x <= 4").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].data.is_empty());
        Ok(())
    }

    #[test]
    fn test_multi_column_two_ranges() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] with index on [a, b]
        let schema = &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)];
        let table_id = db.create_table_for_test_multi_column("test", schema, col_list![0, 1])?;
        let row = product![4u8, 8u8];
        db.with_auto_commit(&ExecutionContext::default(), |tx| db.insert(tx, table_id, row.clone()))?;

        let result = run_for_testing(&db, "select * from test where a >= 3 and a <= 5 and b >= 3 and b <= 5")?;

        let result = result.first().unwrap().clone();
        assert_eq!(result.data, []);

        Ok(())
    }

    #[test]
    fn test_row_limit() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<_, DBError> {
            for i in 0..5u8 {
                db.insert(tx, table_id, product!(i))?;
            }
            Ok(())
        })?;

        let server = Identity::from_hashing_bytes("server");
        let client = Identity::from_hashing_bytes("client");

        let internal_auth = AuthCtx::new(server, server);
        let external_auth = AuthCtx::new(server, client);

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
}
