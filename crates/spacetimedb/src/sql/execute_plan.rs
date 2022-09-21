use super::{
    plan::{Plan, RelationExpr},
    StmtResult,
};
use crate::nodes::worker_node::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb_lib::{TupleDef, TupleValue};

pub fn execute_plan(database_instance_id: u64, plan: Plan) -> Result<StmtResult, anyhow::Error> {
    match plan {
        Plan::Query(query) => match query.source {
            RelationExpr::GetTable { table_id } => execute_get_table(database_instance_id, table_id),
            RelationExpr::Project { input, col_ids } => execute_project(database_instance_id, *input, col_ids),
        },
    }
}

pub fn execute_project(
    database_instance_id: u64,
    input: RelationExpr,
    col_ids: Vec<u32>,
) -> Result<StmtResult, anyhow::Error> {
    // TODO: This is very wrong
    match input {
        RelationExpr::GetTable { table_id } => {
            let mut stmt_result = execute_get_table(database_instance_id, table_id)?;
            stmt_result.rows = stmt_result
                .rows
                .iter()
                .map(|row| TupleValue {
                    elements: row
                        .elements
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| col_ids.contains(&(*i as u32)))
                        .map(|(_, c)| c.clone())
                        .collect::<Vec<_>>(),
                })
                .collect::<Vec<_>>();
            stmt_result.schema = TupleDef {
                elements: stmt_result
                    .schema
                    .elements
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| col_ids.contains(&(*i as u32)))
                    .map(|(_, c)| c.clone())
                    .collect::<Vec<_>>(),
            };
            Ok(stmt_result)
        }
        RelationExpr::Project { input, col_ids } => execute_project(database_instance_id, *input, col_ids),
    }
}

pub fn execute_get_table(database_instance_id: u64, table_id: u32) -> Result<StmtResult, anyhow::Error> {
    let mut rows = Vec::new();
    let database_instance_context = DatabaseInstanceContextController::get_shared()
        .get(database_instance_id)
        .unwrap();
    let mut db = database_instance_context.relational_db.lock().unwrap();
    let mut tx = db.begin_tx();
    for row in db.scan(&mut tx, table_id).unwrap() {
        rows.push(row);
    }
    let schema = db.schema_for_table(&mut tx, table_id).unwrap();
    db.rollback_tx(tx);
    let stmt_result = StmtResult { rows, schema };
    Ok(stmt_result)
}
