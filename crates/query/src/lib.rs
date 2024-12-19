use anyhow::{bail, Result};
use spacetimedb_execution::{execute_plan, Datastore, FallibleDatastore};
use spacetimedb_expr::check::{type_subscription, SchemaView};
use spacetimedb_physical_plan::{compile::compile_sub, plan::ProjectPlan};
use spacetimedb_primitives::TableId;
use spacetimedb_sql_parser::parser::sub::parse_subscription;
use spacetimedb_table::table::RowRef;

pub struct SubscribePlan {
    plan: ProjectPlan,
    #[allow(dead_code)]
    table_id: TableId,
}

impl SubscribePlan {
    pub fn compile(sql: &str, tx: &impl SchemaView) -> Result<Self> {
        let ast = parse_subscription(sql)?;
        let sub = type_subscription(ast, tx)?;
        let Some(table_id) = sub.table_id() else {
            bail!("Failed to get TableId for query plan")
        };
        let plan = compile_sub(sub);
        let plan = plan.optimize();
        Ok(Self { plan, table_id })
    }

    pub fn execute<T: Datastore>(&self, tx: &FallibleDatastore<'_, T>, f: impl FnMut(RowRef)) -> Result<()> {
        execute_plan(&self.plan, tx, f)
    }
}
