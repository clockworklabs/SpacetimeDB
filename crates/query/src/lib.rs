use anyhow::{bail, Result};
use spacetimedb_execution::{
    dml::{MutDatastore, MutExecutor},
    pipelined::ProjectListExecutor,
    Datastore, DeltaStore,
};
use spacetimedb_expr::{
    check::{parse_and_type_sub, SchemaView},
    expr::ProjectList,
    rls::{resolve_views_for_sql, resolve_views_for_sub},
    statement::{parse_and_type_sql, Statement, DML},
};
use spacetimedb_lib::{identity::AuthCtx, metrics::ExecutionMetrics, ProductValue};
use spacetimedb_physical_plan::{
    compile::{compile_dml_plan, compile_select, compile_select_list},
    plan::{ProjectListPlan, ProjectPlan},
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

    Ok((plan_fragments, return_id, return_name, has_param || reads_view))
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
pub fn execute_select_stmt<Tx: Datastore + DeltaStore>(
    auth: &AuthCtx,
    stmt: ProjectList,
    tx: &Tx,
    metrics: &mut ExecutionMetrics,
    check_row_limit: impl Fn(ProjectListPlan) -> Result<ProjectListPlan>,
) -> Result<Vec<ProductValue>> {
    let plan = compile_select_list(stmt).optimize(auth)?;
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
pub fn execute_dml_stmt<Tx: MutDatastore>(
    auth: &AuthCtx,
    stmt: DML,
    tx: &mut Tx,
    metrics: &mut ExecutionMetrics,
) -> Result<()> {
    let plan = compile_dml_plan(stmt).optimize(auth)?;
    let plan = MutExecutor::from(plan);
    plan.execute(tx, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_expr::check::test_utils::graph_schema_viewer;
    use spacetimedb_physical_plan::plan::PhysicalPlan;

    #[test]
    fn compile_cypher_via_sql_direct_path() {
        let tx = graph_schema_viewer();
        let auth = AuthCtx::for_testing();
        let result = compile_sql_stmt("MATCH (a)-[r]->(b) RETURN a", &tx, &auth);
        assert!(result.is_ok(), "Cypher through compile_sql_stmt should succeed: {:?}", result.err());
        let stmt = result.unwrap();
        assert!(matches!(stmt, Statement::Select(_)));
    }

    #[test]
    fn compile_cypher_function_wrapper_via_sql_direct_path() {
        let tx = graph_schema_viewer();
        let auth = AuthCtx::for_testing();
        let result = compile_sql_stmt(
            "SELECT * FROM cypher('MATCH (n:Person) WHERE n.Label = ''Alice'' RETURN n')",
            &tx,
            &auth,
        );
        assert!(result.is_ok(), "cypher() wrapper through compile_sql_stmt should succeed: {:?}", result.err());
    }

    #[test]
    fn compile_cypher_variable_length_via_sql_direct_path() {
        let tx = graph_schema_viewer();
        let auth = AuthCtx::for_testing();
        let result = compile_sql_stmt("MATCH (a)-[*1..3]->(b) RETURN a", &tx, &auth);
        assert!(result.is_ok(), "variable-length Cypher should compile: {:?}", result.err());
    }

    // ── End-to-end graph query integration tests ─────────────────────
    //
    // Each test drives the *full* compilation pipeline:
    //   SQL/Cypher text → parse → type-check → RLS resolution
    //     → physical-plan compilation → optimizer
    //
    // This catches regressions that span crate boundaries — something
    // individual crate-level golden tests cannot detect.

    /// Compile SQL through the full pipeline (compile + physical plan + optimize)
    /// and return the optimized `ProjectListPlan`.
    fn compile_graph_pipeline(sql: &str) -> ProjectListPlan {
        let tx = graph_schema_viewer();
        let auth = AuthCtx::for_testing();
        let stmt = compile_sql_stmt(sql, &tx, &auth)
            .unwrap_or_else(|e| panic!("compilation failed for `{sql}`: {e}"));
        let Statement::Select(project_list) = stmt else {
            panic!("expected Select for `{sql}`, got DML");
        };
        compile_select_list(project_list)
            .optimize(&auth)
            .unwrap_or_else(|e| panic!("optimization failed for `{sql}`: {e}"))
    }

    /// Count join nodes (IxJoin / HashJoin / NLJoin) inside a physical plan tree.
    fn count_physical_joins(plan: &PhysicalPlan) -> usize {
        let mut n = 0;
        plan.visit(&mut |node| match node {
            PhysicalPlan::IxJoin(..) | PhysicalPlan::HashJoin(..) | PhysicalPlan::NLJoin(..) => {
                n += 1;
            }
            _ => {}
        });
        n
    }

    /// Check whether a physical plan tree contains at least one `Filter` node.
    fn has_filter_node(plan: &PhysicalPlan) -> bool {
        let mut found = false;
        plan.visit(&mut |node| {
            if matches!(node, PhysicalPlan::Filter(..)) {
                found = true;
            }
        });
        found
    }

    /// Extract the `Vec<ProjectPlan>` from a `ProjectListPlan::Name` variant.
    fn expect_name_plans<'a>(plan: &'a ProjectListPlan, ctx: &str) -> &'a Vec<ProjectPlan> {
        match plan {
            ProjectListPlan::Name(plans) => plans,
            other => panic!("{ctx}: expected ProjectListPlan::Name, got {other:?}"),
        }
    }

    #[test]
    fn integration_single_hop_full_pipeline() {
        let plan = compile_graph_pipeline("MATCH (a)-[r]->(b) RETURN a");

        let plans = expect_name_plans(&plan, "single-hop");
        assert_eq!(plans.len(), 1, "single-hop produces 1 plan");

        let joins = count_physical_joins(&plans[0]);
        assert!(joins >= 1, "single-hop should contain at least 1 join, got {joins}");
    }

    #[test]
    fn integration_multi_hop_full_pipeline() {
        let plan = compile_graph_pipeline("MATCH (a)-[r]->(b)-[s]->(c) RETURN a");

        let plans = expect_name_plans(&plan, "multi-hop");
        assert_eq!(plans.len(), 1, "fixed-depth 2-hop produces 1 plan");

        let joins = count_physical_joins(&plans[0]);
        assert!(joins >= 2, "2-hop pattern should have at least 2 joins, got {joins}");
    }

    #[test]
    fn integration_variable_length_full_pipeline() {
        let plan = compile_graph_pipeline("MATCH (a)-[*1..3]->(b) RETURN a");

        let plans = expect_name_plans(&plan, "variable-length");
        assert_eq!(
            plans.len(),
            3,
            "[*1..3] should expand to 3 plans (depths 1, 2, 3)"
        );

        for (i, pp) in plans.iter().enumerate() {
            let joins = count_physical_joins(pp);
            assert!(
                joins >= 1,
                "depth-{} plan should have at least 1 join, got {joins}",
                i + 1
            );
        }
    }

    #[test]
    fn integration_label_and_type_filter_full_pipeline() {
        let plan = compile_graph_pipeline(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person) WHERE a.Label = 'Alice' RETURN a",
        );

        let plans = expect_name_plans(&plan, "label-filter");
        assert_eq!(plans.len(), 1, "fixed-depth with labels produces 1 plan");

        assert!(has_filter_node(&plans[0]), "label/type filters should produce at least one Filter node");
    }

    #[test]
    fn integration_cypher_function_wrapper_full_pipeline() {
        let plan = compile_graph_pipeline(
            "SELECT * FROM cypher('MATCH (n:Person) WHERE n.Label = ''Alice'' RETURN n')",
        );

        let plans = expect_name_plans(&plan, "cypher()-wrapper");
        assert_eq!(plans.len(), 1, "cypher() wrapper on single node produces 1 plan");
    }

    #[test]
    fn integration_property_projection_full_pipeline() {
        let plan = compile_graph_pipeline("MATCH (a)-[r]->(b) RETURN a.Label, b.Label");

        match &plan {
            ProjectListPlan::List(physical_plans, fields) => {
                assert!(!physical_plans.is_empty(), "should have at least 1 physical plan");
                assert_eq!(fields.len(), 2, "RETURN a.Label, b.Label should produce 2 output fields");
            }
            other => panic!("expected ProjectListPlan::List for column projection, got {other:?}"),
        }
    }

    #[test]
    fn integration_variable_length_with_label_filter_full_pipeline() {
        let plan = compile_graph_pipeline("MATCH (a:Person)-[*1..2]->(b) RETURN a");

        let plans = expect_name_plans(&plan, "vlp-with-label");
        assert_eq!(
            plans.len(),
            2,
            "[*1..2] with label filter should produce 2 plans"
        );
    }

    #[test]
    fn integration_incoming_direction_full_pipeline() {
        let plan = compile_graph_pipeline("MATCH (a)<-[r]-(b) RETURN a");

        let plans = expect_name_plans(&plan, "incoming");
        assert_eq!(plans.len(), 1, "incoming single-hop produces 1 plan");

        let joins = count_physical_joins(&plans[0]);
        assert!(joins >= 1, "incoming hop should have at least 1 join, got {joins}");
    }

    #[test]
    fn integration_where_clause_full_pipeline() {
        let plan = compile_graph_pipeline(
            "MATCH (a)-[r]->(b) WHERE b.Label = 'Server' RETURN a",
        );

        let plans = expect_name_plans(&plan, "where-clause");
        assert_eq!(plans.len(), 1, "single-hop with WHERE produces 1 plan");

        assert!(has_filter_node(&plans[0]), "WHERE clause should produce a Filter node in the physical plan");
    }

    #[test]
    fn integration_not_in_where_clause_full_pipeline() {
        let plan = compile_graph_pipeline(
            "MATCH (a)-[r]->(b) WHERE NOT b.Label = 'Bot' RETURN a",
        );

        let plans = expect_name_plans(&plan, "not-where");
        assert_eq!(plans.len(), 1, "single-hop with NOT WHERE produces 1 plan");

        assert!(has_filter_node(&plans[0]), "NOT in WHERE should produce a Filter node");
    }

    #[test]
    fn integration_not_combined_with_and_full_pipeline() {
        let plan = compile_graph_pipeline(
            "MATCH (a)-[r]->(b) WHERE a.Label = 'Alice' AND NOT b.Label = 'Bot' RETURN a",
        );

        let plans = expect_name_plans(&plan, "not-and-where");
        assert_eq!(plans.len(), 1, "AND with NOT produces 1 plan");

        assert!(has_filter_node(&plans[0]), "AND+NOT in WHERE should produce a Filter node");
    }
}
