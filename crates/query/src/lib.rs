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
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, sats::u256, ProductValue};
use spacetimedb_physical_plan::{
    compile::{compile_dml_plan, compile_select, compile_select_list},
    plan::{ProjectListPlan, ProjectPlan},
};
use spacetimedb_primitives::{TableId, ViewId};
use spacetimedb_schema::table_name::TableName;

/// DIRTY HACK ALERT: Maximum allowed length, in UTF-8 bytes, of SQL queries.
/// Any query longer than this will be rejected.
/// This prevents a stack overflow when compiling queries with deeply-nested `AND` and `OR` conditions.
const MAX_SQL_LENGTH: usize = 50_000;

/// A subscription query compiled into plan fragments.
pub struct CompiledSubscription {
    pub plans: Vec<ProjectPlan>,
    pub return_id: TableId,
    pub return_name: TableName,
    /// Does the result of this subscription depend on the caller's identity,
    /// i.e. does it use `:sender`, row-level security, or a view materialized per caller?
    pub reads_sender: bool,
    /// Does this subscription read from a scoped view?
    /// If so, its result depends on the caller's scope for each such view.
    pub reads_scoped_view: bool,
}

/// Compile a subscription query.
///
/// The returned `bool` is set if the result of the subscription may depend on the caller,
/// either through its identity or through its scopes.
pub fn compile_subscription(
    sql: &str,
    tx: &impl SchemaView,
    auth: &AuthCtx,
) -> Result<(Vec<ProjectPlan>, TableId, TableName, bool)> {
    let CompiledSubscription {
        plans,
        return_id,
        return_name,
        reads_sender,
        reads_scoped_view,
    } = compile_subscription_detailed(sql, tx, auth)?;
    Ok((plans, return_id, return_name, reads_sender || reads_scoped_view))
}

/// Compile a subscription query,
/// distinguishing whether its result depends on the caller's identity or on their scopes.
pub fn compile_subscription_detailed(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<CompiledSubscription> {
    if sql.len() > MAX_SQL_LENGTH {
        bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
    }

    let (plan, mut has_param) = parse_and_type_sub(sql, tx, auth)?;

    let Some(return_id) = plan.return_table_id() else {
        bail!("Failed to determine TableId for query")
    };

    let Some(return_name) = tx.schema_for_table(return_id).map(|schema| schema.table_name.clone()) else {
        bail!("TableId `{return_id}` does not exist")
    };

    // Resolve any RLS filters
    let plan_fragments = resolve_views_for_sub(tx, plan, auth, &mut has_param)?
        .into_iter()
        .map(compile_select)
        .collect::<Vec<_>>();

    // Does this subscription read from a client-specific view?
    // If so, it is as if the view is parameterized by `:sender`.
    // We must know this in order to generate the correct query hash.
    let reads_view = plan_fragments.iter().any(|plan| plan.reads_from_view(false));

    // Does this subscription read from a scoped view?
    // If so, it is as if the view is parameterized by the caller's scope.
    let reads_scoped_view = plan_fragments.iter().any(|plan| !plan.scoped_view_ids().is_empty());

    Ok(CompiledSubscription {
        plans: plan_fragments,
        return_id,
        return_name,
        reads_sender: has_param || reads_view,
        reads_scoped_view,
    })
}

/// A utility for parsing and type checking a sql statement
pub fn compile_sql_stmt(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> Result<Statement> {
    if sql.len() > MAX_SQL_LENGTH {
        bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
    }

    match parse_and_type_sql(sql, tx, auth)? {
        stmt @ Statement::DML(_) => Ok(stmt),
        Statement::Select(expr) => Ok(Statement::Select(resolve_views_for_sql(tx, expr, auth)?)),
    }
}

/// A utility for executing a sql select statement
///
/// Scoped views read by the statement select the rows of the caller's scopes `view_scopes`.
pub fn execute_select_stmt<Tx: Datastore + DeltaStore>(
    auth: &AuthCtx,
    stmt: ProjectList,
    view_scopes: Vec<(ViewId, u256)>,
    tx: &Tx,
    metrics: &mut ExecutionMetrics,
    check_row_limit: impl Fn(ProjectListPlan) -> Result<ProjectListPlan>,
) -> Result<Vec<ProductValue>> {
    let plan = compile_select_list(stmt).optimize()?;
    let plan = check_row_limit(plan)?;
    let plan = ProjectListExecutor::from(plan);
    let params = ExecutionParams::from_auth(auth).with_view_scopes(view_scopes);
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
