use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{ProductType, ProductValue};
use spacetimedb_sats::relation::MemTable;
use spacetimedb_vm::eval::run_ast;
use spacetimedb_vm::expr::{CodeResult, CrudExpr, Expr};
use tracing::info;

use crate::database_instance_context_controller::DatabaseInstanceContextController;
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::{DBError, DatabaseError};
use crate::execution_context::ExecutionContext;
use crate::sql::compiler::compile_sql;
use crate::vm::{DbProgram, TxMode};

pub struct StmtResult {
    pub schema: ProductType,
    pub rows: Vec<ProductValue>,
}

// TODO(cloutiertyler): we could do this the swift parsing way in which
// we always generate a plan, but it may contain errors

/// Run a `SQL` query/statement in the specified `database_instance_id`.
#[tracing::instrument(skip_all)]
pub fn execute(
    db_inst_ctx_controller: &DatabaseInstanceContextController,
    database_instance_id: u64,
    sql_text: String,
    auth: AuthCtx,
) -> Result<Vec<MemTable>, DBError> {
    info!(sql = sql_text);
    if let Some((database_instance_context, _)) = db_inst_ctx_controller.get(database_instance_id) {
        run(&database_instance_context.relational_db, &sql_text, auth)
    } else {
        Err(DatabaseError::NotFound(database_instance_id).into())
    }
}

fn collect_result(result: &mut Vec<MemTable>, r: CodeResult) -> Result<(), DBError> {
    match r {
        CodeResult::Value(_) => {}
        CodeResult::Table(x) => result.push(x),
        CodeResult::Block(lines) => {
            for x in lines {
                collect_result(result, x)?;
            }
        }
        CodeResult::Halt(err) => return Err(DBError::VmUser(err)),
        CodeResult::Pass => {}
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
pub fn execute_single_sql(
    cx: &ExecutionContext,
    db: &RelationalDB,
    tx: &mut Tx,
    ast: CrudExpr,
    auth: AuthCtx,
) -> Result<Vec<MemTable>, DBError> {
    let mut tx: TxMode = tx.into();
    let p = &mut DbProgram::new(cx, db, &mut tx, auth);
    let q = Expr::Crud(Box::new(ast));

    let mut result = Vec::with_capacity(1);
    collect_result(&mut result, run_ast(p, q).into())?;
    Ok(result)
}

#[tracing::instrument(skip_all)]
pub fn execute_sql_mut_tx(
    db: &RelationalDB,
    tx: &mut MutTx,
    ast: Vec<CrudExpr>,
    auth: AuthCtx,
) -> Result<Vec<MemTable>, DBError> {
    let total = ast.len();
    let mut tx: TxMode = tx.into();
    let ctx = ExecutionContext::sql(db.address());
    let p = &mut DbProgram::new(&ctx, db, &mut tx, auth);
    let q = Expr::Block(ast.into_iter().map(|x| Expr::Crud(Box::new(x))).collect());

    let mut result = Vec::with_capacity(total);
    collect_result(&mut result, run_ast(p, q).into())?;
    Ok(result)
}

/// Run the compiled `SQL` expression inside the `vm` created by [DbProgram]
///
/// Evaluates `ast` and accordingly triggers mutable or read tx to execute
#[tracing::instrument(skip_all)]
pub fn execute_sql(db: &RelationalDB, ast: Vec<CrudExpr>, auth: AuthCtx) -> Result<Vec<MemTable>, DBError> {
    let total = ast.len();
    let ctx = ExecutionContext::sql(db.address());
    let mut result = Vec::with_capacity(total);
    match CrudExpr::is_reads(&ast) {
        false => db.with_auto_commit(&ctx, |mut_tx| {
            let mut tx: TxMode = mut_tx.into();
            let q = Expr::Block(ast.into_iter().map(|x| Expr::Crud(Box::new(x))).collect());
            let p = &mut DbProgram::new(&ctx, db, &mut tx, auth);
            collect_result(&mut result, run_ast(p, q).into())
        }),
        true => db.with_read_only(&ctx, |tx| {
            let mut tx: TxMode = tx.into();
            let q = Expr::Block(ast.into_iter().map(|x| Expr::Crud(Box::new(x))).collect());
            let p = &mut DbProgram::new(&ctx, db, &mut tx, auth);
            collect_result(&mut result, run_ast(p, q).into())
        }),
    }?;

    Ok(result)
}

/// Run the `SQL` string using the `auth` credentials
#[tracing::instrument(skip_all)]
pub fn run(db: &RelationalDB, sql_text: &str, auth: AuthCtx) -> Result<Vec<MemTable>, DBError> {
    let ctx = &ExecutionContext::sql(db.address());
    let ast = db.with_read_only(ctx, |tx| compile_sql(db, tx, sql_text))?;
    execute_sql(db, ast, auth)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{ST_TABLES_ID, ST_TABLES_NAME};
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::vm::tests::create_table_with_rows;
    use itertools::Itertools;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::relation::{Header, RelValue};
    use spacetimedb_sats::{product, AlgebraicType, ProductType};
    use spacetimedb_vm::dsl::{mem_table, scalar};
    use spacetimedb_vm::eval::create_game_data;
    use tempfile::TempDir;

    /// Short-cut for simplify test execution
    fn run_for_testing(db: &RelationalDB, sql_text: &str) -> Result<Vec<MemTable>, DBError> {
        run(db, sql_text, AuthCtx::for_testing())
    }

    fn create_data(total_rows: u64) -> ResultTest<(RelationalDB, MemTable, TempDir)> {
        let (db, tmp_dir) = make_test_db()?;

        let mut tx = db.begin_mut_tx();
        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let rows: Vec<_> = (1..=total_rows).map(|i| product!(i, format!("health{i}"))).collect();
        create_table_with_rows(&db, &mut tx, "inventory", head.clone(), &rows)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        Ok((db, mem_table(head, rows), tmp_dir))
    }

    #[test]
    fn test_select_star() -> ResultTest<()> {
        let (db, input, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_select_star_table() -> ResultTest<()> {
        let (db, input, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory.* FROM inventory")?;
        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
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
        let input = mem_table(head, vec![row]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );

        Ok(())
    }

    #[test]
    fn test_select_scalar() -> ResultTest<()> {
        let (db, _, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT 1 FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();
        let schema = ProductType::from([AlgebraicType::I32]);
        let row = product!(1);
        let input = mem_table(schema, vec![row]);

        assert_eq!(result.as_without_table_name(), input.as_without_table_name(), "Scalar");
        Ok(())
    }

    #[test]
    fn test_select_multiple() -> ResultTest<()> {
        let (db, input, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT * FROM inventory;\nSELECT * FROM inventory")?;

        assert_eq!(result.len(), 2, "Not return results");

        for x in result {
            assert_eq!(x.as_without_table_name(), input.as_without_table_name(), "Inventory");
        }
        Ok(())
    }

    #[test]
    fn test_select_catalog() -> ResultTest<()> {
        let (db, _, _tmp_dir) = create_data(1)?;
        let tx = db.begin_tx();
        let schema = db.schema_for_table(&tx, ST_TABLES_ID).unwrap().into_owned();
        db.release_tx(&ExecutionContext::internal(db.address()), tx);
        let result = run_for_testing(
            &db,
            &format!("SELECT * FROM {} WHERE table_id = {}", ST_TABLES_NAME, ST_TABLES_ID),
        )?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();
        let row = product!(
            scalar(ST_TABLES_ID),
            scalar(ST_TABLES_NAME),
            scalar(StTableType::System.as_str()),
            scalar(StAccess::Public.as_str()),
        );
        let input = mem_table(Header::from(&schema), vec![row]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "st_table"
        );
        Ok(())
    }

    #[test]
    fn test_select_column() -> ResultTest<()> {
        let (db, table, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory_id FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();
        //The expected result
        let col = table.head.find_by_name("inventory_id").unwrap();
        let inv = table.head.project(&[col.field.clone()]).unwrap();

        let row = product!(scalar(1u64));
        let input = mem_table(inv, vec![row]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_where() -> ResultTest<()> {
        let (db, table, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(&db, "SELECT inventory_id FROM inventory WHERE inventory_id = 1")?;

        assert_eq!(result.len(), 1, "Not return results");
        let result = result.first().unwrap().clone();

        //The expected result
        let col = table.head.find_by_name("inventory_id").unwrap();
        let inv = table.head.project(&[col.field.clone()]).unwrap();

        let row = product!(scalar(1u64));
        let input = mem_table(inv, vec![row]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_or() -> ResultTest<()> {
        let (db, table, _tmp_dir) = create_data(2)?;

        let result = run_for_testing(
            &db,
            "SELECT inventory_id FROM inventory WHERE inventory_id = 1 OR inventory_id = 2",
        )?;

        assert_eq!(result.len(), 1, "Not return results");
        let mut result = result.first().unwrap().clone();
        result.data.sort();
        //The expected result
        let col = table.head.find_by_name("inventory_id").unwrap();
        let inv = table.head.project(&[col.field.clone()]).unwrap();

        let input = mem_table(inv, vec![product!(scalar(1u64)), product!(scalar(2u64))]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_nested() -> ResultTest<()> {
        let (db, table, _tmp_dir) = create_data(2)?;

        let result = run_for_testing(
            &db,
            "SELECT (inventory_id) FROM inventory WHERE (inventory_id = 1 OR inventory_id = 2 AND (1=1))",
        )?;

        assert_eq!(result.len(), 1, "Not return results");
        let mut result = result.first().unwrap().clone();
        result.data.sort();
        //The expected result
        let col = table.head.find_by_name("inventory_id").unwrap();
        let inv = table.head.project(&[col.field.clone()]).unwrap();

        let input = mem_table(inv, vec![product!(scalar(1u64)), product!(scalar(2u64))]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );
        Ok(())
    }

    #[test]
    fn test_inner_join() -> ResultTest<()> {
        let data = create_game_data();

        let (db, _tmp_dir) = make_test_db()?;

        let mut tx = db.begin_mut_tx();
        create_table_with_rows(
            &db,
            &mut tx,
            "Inventory",
            data.inv.head.into(),
            &data.inv.data.iter().map(|row| row.data.clone()).collect_vec(),
        )?;
        create_table_with_rows(
            &db,
            &mut tx,
            "Player",
            data.player.head.into(),
            &data.player.data.iter().map(|row| row.data.clone()).collect_vec(),
        )?;
        create_table_with_rows(
            &db,
            &mut tx,
            "Location",
            data.location.head.into(),
            &data.location.data.iter().map(|row| row.data.clone()).collect_vec(),
        )?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

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

        let head = ProductType::from([("entity_id", AlgebraicType::U64), ("inventory_id", AlgebraicType::U64)]);
        let row1 = product!(100u64, 1u64);
        let input = mem_table(head, [row1]);

        assert_eq!(
            input.as_without_table_name(),
            result.as_without_table_name(),
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

        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row1 = product!(1u64, "health");
        let input = mem_table(head, [row1]);

        assert_eq!(
            input.as_without_table_name(),
            result.as_without_table_name(),
            "Inventory JOIN Player JOIN Location"
        );
        Ok(())
    }

    #[test]
    fn test_insert() -> ResultTest<()> {
        let (db, mut input, _tmp_dir) = create_data(1)?;
        let result = run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 'test')")?;

        assert_eq!(result.len(), 0, "Return results");

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        assert_eq!(result.len(), 1, "Not return results");
        let mut result = result.first().unwrap().clone();

        let row = product!(scalar(2u64), scalar("test"));
        input.data.push(RelValue::new(row, None));
        input.data.sort();
        result.data.sort();

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );

        Ok(())
    }

    #[test]
    fn test_delete() -> ResultTest<()> {
        let (db, _input, _tmp_dir) = create_data(1)?;

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
        let (db, input, _tmp_dir) = create_data(1)?;

        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (2, 't2')")?;
        run_for_testing(&db, "INSERT INTO inventory (inventory_id, name) VALUES (3, 't3')")?;

        run_for_testing(&db, "UPDATE inventory SET name = 'c2' WHERE inventory_id = 2")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory WHERE inventory_id = 2")?;

        let result = result.first().unwrap().clone();
        let row = product!(scalar(2u64), scalar("c2"));

        let mut change = input;
        change.data.clear();
        change.data.push(RelValue::new(row, None));

        assert_eq!(
            change.as_without_table_name(),
            result.as_without_table_name(),
            "Update Inventory 2"
        );

        run_for_testing(&db, "UPDATE inventory SET name = 'c3'")?;

        let result = run_for_testing(&db, "SELECT * FROM inventory")?;

        let updated: Vec<_> = result
            .into_iter()
            .map(|x| {
                x.data
                    .into_iter()
                    .map(|x| x.data.field_as_str(1, None).unwrap().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(vec![vec!["c3"; 3]], updated);

        Ok(())
    }

    #[test]
    fn test_create_table() -> ResultTest<()> {
        let (db, _, _tmp_dir) = create_data(1)?;
        run_for_testing(&db, "CREATE TABLE inventory2 (inventory_id BIGINT UNSIGNED, name TEXT)")?;
        run_for_testing(
            &db,
            "INSERT INTO inventory2 (inventory_id, name) VALUES (1, 'health1') ",
        )?;

        let a = run_for_testing(&db, "SELECT * FROM inventory")?;
        let a = a.first().unwrap().clone();

        let b = run_for_testing(&db, "SELECT * FROM inventory2")?;
        let b = b.first().unwrap().clone();

        assert_eq!(a.as_without_table_name(), b.as_without_table_name(), "Inventory");

        Ok(())
    }

    #[test]
    fn test_drop_table() -> ResultTest<()> {
        let (db, _, _tmp_dir) = create_data(1)?;
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
        let (db, _, _tmp_dir) = create_data(0)?;

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
        let (db, _input, _tmp_dir) = create_data(1)?;

        let result = run_for_testing(
            &db,
            "insert into inventory (id, name) values (1, 'Kiley');
insert into inventory (id, name) values (2, 'Terza');
insert into inventory (id, name) values (3, 'Alvie');
SELECT * FROM inventory",
        )?;

        let result = result.first().unwrap().clone();
        assert_eq!(result.data.len(), 4);

        Ok(())
    }
}
