use std::ops::Deref;

use anyhow::{bail, Result};
use delta::DeltaPlan;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    Compression, DatabaseUpdate, QueryUpdate, TableUpdate, WebsocketFormat,
};
use spacetimedb_execution::{pipelined::PipelinedProject, Datastore, DeltaStore};
use spacetimedb_expr::check::{type_subscription, SchemaView};
use spacetimedb_physical_plan::{compile::compile_project_plan, plan::ProjectPlan};
use spacetimedb_primitives::TableId;
use spacetimedb_sql_parser::parser::sub::parse_subscription;

pub mod delta;

/// DIRTY HACK ALERT: Maximum allowed length, in UTF-8 bytes, of SQL queries.
/// Any query longer than this will be rejected.
/// This prevents a stack overflow when compiling queries with deeply-nested `AND` and `OR` conditions.
const MAX_SQL_LENGTH: usize = 50_000;

/// A subscription query plan that is NOT used for incremental evaluation
#[derive(Debug)]
pub struct SubscribePlan {
    /// The query plan
    plan: ProjectPlan,
    /// Table id of the returned rows
    table_id: TableId,
    /// Table name of the returned rows
    table_name: Box<str>,
}

impl Deref for SubscribePlan {
    type Target = ProjectPlan;

    fn deref(&self) -> &Self::Target {
        &self.plan
    }
}

impl SubscribePlan {
    /// Subscription queries always return rows from a single table
    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    /// Subscription queries always return rows from a single table
    pub fn table_name(&self) -> &str {
        self.table_name.as_ref()
    }

    /// Delta plans are only materialized, and optimized, at runtime.
    /// Hence we are free to instantiate a non-delta plans from them.
    pub fn from_delta_plan(plan: &DeltaPlan) -> Self {
        let table_id = plan.table_id();
        let table_name = plan.table_name();
        let plan = &**plan;
        let plan = plan.clone().optimize();
        Self {
            plan,
            table_id,
            table_name,
        }
    }

    /// Compile a subscription query for standard execution
    pub fn compile(sql: &str, tx: &impl SchemaView) -> Result<Self> {
        if sql.len() > MAX_SQL_LENGTH {
            bail!("SQL query exceeds maximum allowed length: \"{sql:.120}...\"")
        }
        let ast = parse_subscription(sql)?;
        let sub = type_subscription(ast, tx)?;

        let Some(table_id) = sub.table_id() else {
            bail!("Failed to determine TableId for query")
        };

        let Some(table_name) = tx.schema_for_table(table_id).map(|schema| schema.table_name.clone()) else {
            bail!("TableId `{table_id}` does not exist")
        };

        let plan = compile_project_plan(sub);
        let plan = plan.optimize();

        Ok(Self {
            plan,
            table_id,
            table_name,
        })
    }

    /// Execute a subscription query
    pub fn execute<Tx, F>(&self, tx: &Tx) -> Result<(F::List, u64)>
    where
        Tx: Datastore + DeltaStore,
        F: WebsocketFormat,
    {
        let plan = PipelinedProject::from(self.plan.clone());
        let mut rows = vec![];
        plan.execute(tx, &mut |row| {
            rows.push(row);
            Ok(())
        })?;
        Ok(F::encode_list(rows.into_iter()))
    }

    /// Execute a subscription query and collect the results in a [TableUpdate]
    pub fn collect_table_update<Tx, F>(&self, comp: Compression, tx: &Tx) -> Result<TableUpdate<F>>
    where
        Tx: Datastore + DeltaStore,
        F: WebsocketFormat,
    {
        self.execute::<Tx, F>(tx).map(|(inserts, num_rows)| {
            let deletes = F::List::default();
            let qu = QueryUpdate { deletes, inserts };
            let update = F::into_query_update(qu, comp);
            TableUpdate::new(self.table_id, self.table_name.clone(), (update, num_rows))
        })
    }
}

/// Execute a collection of subscription queries in parallel
pub fn execute_plans<Tx, F>(plans: Vec<SubscribePlan>, comp: Compression, tx: &Tx) -> Result<DatabaseUpdate<F>>
where
    Tx: Datastore + DeltaStore + Sync,
    F: WebsocketFormat,
{
    plans
        .par_iter()
        .map(|plan| plan.collect_table_update(comp, tx))
        .collect::<Result<_>>()
        .map(|tables| DatabaseUpdate { tables })
}
