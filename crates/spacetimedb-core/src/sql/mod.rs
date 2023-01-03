mod execute_plan;
mod plan;
mod plan_statement;

use spacetimedb_lib::{TupleDef, TupleValue};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

pub struct StmtResult {
    pub schema: TupleDef,
    pub rows: Vec<TupleValue>,
}

pub fn execute(
    database_instance_id: u64,
    sql_text: String,
) -> Result<Vec<Result<StmtResult, anyhow::Error>>, anyhow::Error> {
    let dialect = GenericDialect {}; // or AnsiDialect
    let ast = Parser::parse_sql(&dialect, &sql_text)?;

    let mut results: Vec<Result<StmtResult, _>> = Vec::new();
    for statement in ast {
        let plan_result = plan_statement::plan_statement(database_instance_id, statement);
        let plan = match plan_result {
            Ok(plan) => plan,
            Err(err) => {
                results.push(Err(err.into()));
                continue;
            }
        };
        results.push(execute_plan::execute_plan(database_instance_id, plan));
    }
    Ok(results)
}
