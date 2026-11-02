use anyhow::{bail, Result};
use spacetimedb_execution::{
    dml::{MutDatastore, MutExecutor},
    pipelined::ProjectListExecutor,
    Datastore, DeltaStore, ExecutionParams,
};
use spacetimedb_expr::{
    check::{parse_and_type_sub, SchemaView},
    expr::ProjectList,
    rls::{resolve_views_for_sql, resolve_views_for_sub},
    statement::{parse_and_type_sql, Statement, DML},
};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, AlgebraicType, ProductValue};
use spacetimedb_physical_plan::{
    compile::{compile_dml_plan, compile_select, compile_select_list},
    plan::{ParamSource, ProjectListPlan, ProjectPlan},
};
use spacetimedb_primitives::TableId;
use spacetimedb_schema::table_name::TableName;

/// DIRTY HACK ALERT: Maximum allowed length, in UTF-8 bytes, of SQL queries.
/// Any query longer than this will be rejected.
/// This prevents a stack overflow when compiling queries with deeply-nested `AND` and `OR` conditions.
const MAX_SQL_LENGTH: usize = 50_000;

pub fn compile_subscription(
    sql: &str,
    tx: &impl SchemaView,
    auth: &AuthCtx,
) -> Result<(Vec<ProjectPlan>, TableId, TableName, bool)> {
    let SubscriptionTemplate {
        plans,
        return_id,
        return_name,
        has_param,
        ..
    } = compile(sql, tx, auth, false)?;
    Ok((plans, return_id, return_name, has_param))
}

/// A subscription compiled as a template, whose plans read every value from a binding
pub struct SubscriptionTemplate {
    /// One plan per RLS fragment, not yet optimized
    pub plans: Vec<ProjectPlan>,
    /// The source and type of each binding param, in slot order
    pub params: Vec<(ParamSource, AlgebraicType)>,
    pub return_id: TableId,
    pub return_name: TableName,
    pub has_param: bool,
}

/// Like [`compile_subscription`], but lifts each `field = literal` and built-in into a binding param
pub fn compile_subscription_template(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<SubscriptionTemplate> {
    compile(sql, tx, auth, true)
}

fn compile(sql: &str, tx: &impl SchemaView, auth: &AuthCtx, template: bool) -> Result<SubscriptionTemplate> {
    if sql.len() > MAX_SQL_LENGTH {
        bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
    }

    let (mut plan, mut has_param) = parse_and_type_sub(sql, tx, auth)?;

    // Before RLS, so the literals of its rules stay literal
    let literals = if template { plan.lift_literals() } else { vec![] };
    let mut params: Vec<_> = literals
        .into_iter()
        .map(|(value, ty)| (ParamSource::Literal(value), ty))
        .collect();

    let Some(return_id) = plan.return_table_id() else {
        bail!("Failed to determine TableId for query")
    };

    let Some(return_name) = tx.schema_for_table(return_id).map(|schema| schema.table_name.clone()) else {
        bail!("TableId `{return_id}` does not exist")
    };

    // Resolve any RLS filters
    let plan_fragments = resolve_views_for_sub(tx, plan, auth, &mut has_param)?
        .into_iter()
        .map(|mut fragment| {
            fragment.bind_view_arg_hashes(auth.caller());
            let mut plan = compile_select(fragment);
            // The fragments share the lifted literals, so they share the built-ins' slots too
            if template {
                plan.remap_builtin_params(&mut params);
            }
            plan
        })
        .collect::<Vec<_>>();

    // Does this subscription read from a client-specific view?
    // If so, it is as if the view is parameterized by `:sender`.
    // We must know this in order to generate the correct query hash.
    let reads_view = plan_fragments.iter().any(|plan| plan.reads_from_view(false));

    Ok(SubscriptionTemplate {
        plans: plan_fragments,
        params,
        return_id,
        return_name,
        has_param: has_param || reads_view,
    })
}

/// A utility for parsing and type checking a sql statement
pub fn compile_sql_stmt(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<Statement> {
    if sql.len() > MAX_SQL_LENGTH {
        bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
    }

    match parse_and_type_sql(sql, tx, auth)? {
        stmt @ Statement::DML(_) => Ok(stmt),
        Statement::Select(expr) => {
            let mut expr = resolve_views_for_sql(tx, expr, auth)?;
            expr.bind_view_arg_hashes(auth.caller());
            Ok(Statement::Select(expr))
        }
    }
}

/// A utility for executing a sql select statement
pub fn execute_select_stmt<Tx: Datastore + DeltaStore>(
    auth: &AuthCtx,
    stmt: ProjectList,
    tx: &Tx,
    metrics: &mut ExecutionMetrics,
    check_row_limit: impl Fn(ProjectListPlan) -> Result<ProjectListPlan>,
) -> Result<Vec<ProductValue>> {
    let plan = compile_select_list(stmt).optimize()?;
    let plan = check_row_limit(plan)?;
    let plan = ProjectListExecutor::from(plan);
    let params = ExecutionParams::from_auth(auth);
    let mut rows = vec![];
    plan.execute(tx, &params, metrics, &mut |row| {
        rows.push(row);
        Ok(())
    })?;
    Ok(rows)
}

/// A utility for executing a sql dml statement
pub fn execute_dml_stmt<Tx: MutDatastore>(
    auth: &AuthCtx,
    stmt: DML,
    tx: &mut Tx,
    metrics: &mut ExecutionMetrics,
) -> Result<()> {
    let plan = compile_dml_plan(stmt).optimize()?;
    let plan = MutExecutor::from(plan);
    plan.execute(tx, &ExecutionParams::from_auth(auth), metrics)
}
