use anyhow::{bail, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    Compression, DatabaseUpdate, QueryUpdate, TableUpdate, WebsocketFormat,
};
use spacetimedb_execution::{execute_plan, iter::PlanIter, Datastore};
use spacetimedb_expr::check::{type_subscription, SchemaView};
use spacetimedb_physical_plan::{compile::compile_sub, plan::ProjectPlan};
use spacetimedb_primitives::TableId;
use spacetimedb_sql_parser::parser::sub::parse_subscription;

pub struct SubscribePlan {
    /// The query plan
    plan: ProjectPlan,
    /// Table id of the returned rows
    table_id: TableId,
    /// Table name of the returned rows
    table_name: Box<str>,
}

impl SubscribePlan {
    pub fn compile(sql: &str, tx: &impl SchemaView) -> Result<Self> {
        let ast = parse_subscription(sql)?;
        let sub = type_subscription(ast, tx)?;

        let Some(table_id) = sub.table_id() else {
            bail!("Failed to determine TableId for query")
        };

        let Some(table_name) = tx.schema_for_table(table_id).map(|schema| schema.table_name.clone()) else {
            bail!("TableId `{table_id}` does not exist")
        };

        let plan = compile_sub(sub);
        let plan = plan.optimize();
        Ok(Self {
            plan,
            table_id,
            table_name,
        })
    }

    pub fn execute<F: WebsocketFormat>(&self, comp: Compression, tx: &impl Datastore) -> Result<TableUpdate<F>> {
        execute_plan(&self.plan, tx, |iter| match iter {
            PlanIter::Index(iter) => F::encode_list(iter),
            PlanIter::Table(iter) => F::encode_list(iter),
            PlanIter::RowId(iter) => F::encode_list(iter),
            PlanIter::Tuple(iter) => F::encode_list(iter),
        })
        .map(|(inserts, num_rows)| {
            let deletes = F::List::default();
            let qu = QueryUpdate { deletes, inserts };
            let update = F::into_query_update(qu, comp);
            TableUpdate::new(self.table_id, self.table_name.clone(), (update, num_rows))
        })
    }
}

pub fn execute_plans<F, Tx>(plans: Vec<SubscribePlan>, comp: Compression, tx: &Tx) -> Result<DatabaseUpdate<F>>
where
    F: WebsocketFormat,
    Tx: Datastore + Sync,
{
    plans
        .par_iter()
        .map(|plan| plan.execute(comp, tx))
        .collect::<Result<_>>()
        .map(|tables| DatabaseUpdate { tables })
}