use anyhow::{bail, Result};
use spacetimedb_execution::{
    dml::{MutDatastore, MutExecutor},
    pipelined::ProjectListExecutor,
    Datastore, DeltaStore,
};
use spacetimedb_expr::{
    check::{parse_and_type_sub, SchemaView},
    expr::ProjectList,
    statement::{parse_and_type_sql, Statement, DML},
};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, ProductValue};
use spacetimedb_physical_plan::{
    compile::{compile_dml_plan, compile_select, compile_select_list},
    plan::{ProjectListPlan, ProjectPlan},
};
use spacetimedb_primitives::TableId;

/// DIRTY HACK ALERT: Maximum allowed length, in UTF-8 bytes, of SQL queries.
/// Any query longer than this will be rejected.
/// This prevents a stack overflow when compiling queries with deeply-nested `AND` and `OR` conditions.
const MAX_SQL_LENGTH: usize = 50_000;

pub fn compile_subscription(
    sql: &str,
    tx: &impl SchemaView,
    auth: &AuthCtx,
) -> Result<(ProjectPlan, TableId, Box<str>, bool)> {
    if sql.len() > MAX_SQL_LENGTH {
        bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
    }

    let (plan, has_param) = parse_and_type_sub(sql, tx, auth)?;

    let Some(return_id) = plan.return_table_id() else {
        bail!("Failed to determine TableId for query")
    };

    let Some(return_name) = tx.schema_for_table(return_id).map(|schema| schema.table_name.clone()) else {
        bail!("TableId `{return_id}` does not exist")
    };

    let plan = compile_select(plan);

    Ok((plan, return_id, return_name, has_param))
}

/// A utility for parsing and type checking a sql statement
pub fn compile_sql_stmt(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<Statement> {
    if sql.len() > MAX_SQL_LENGTH {
        bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
    }
    Ok(parse_and_type_sql(sql, tx, auth)?)
}

/// A utility for executing a sql select statement
pub fn execute_select_stmt<Tx: Datastore + DeltaStore>(
    stmt: ProjectList,
    tx: &Tx,
    metrics: &mut ExecutionMetrics,
    check_row_limit: impl Fn(ProjectListPlan) -> Result<ProjectListPlan>,
) -> Result<Vec<ProductValue>> {
    let plan = compile_select_list(stmt).optimize()?;
    let plan = check_row_limit(plan)?;
    let plan = ProjectListExecutor::from(plan);
    let mut rows = vec![];
    plan.execute(tx, metrics, &mut |row| {
        rows.push(row);
        Ok(())
    })?;
    Ok(rows)
}

/// A utility for executing a sql dml statement
pub fn execute_dml_stmt<Tx: MutDatastore>(stmt: DML, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<()> {
    let plan = compile_dml_plan(stmt).optimize()?;
    let plan = MutExecutor::from(plan);
    plan.execute(tx, metrics)
}
