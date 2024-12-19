use anyhow::Result;
use spacetimedb_client_api_messages::websocket::{BsatnFormat, BsatnRowList, WebsocketFormat};
use spacetimedb_execution::{execute_plan, iter::PlanIter, Datastore};
use spacetimedb_expr::check::{type_subscription, SchemaView};
use spacetimedb_physical_plan::{compile::compile_sub, plan::ProjectPlan};
use spacetimedb_sql_parser::parser::sub::parse_subscription;

pub struct SubscribePlan {
    plan: ProjectPlan,
}

impl SubscribePlan {
    pub fn compile(sql: &str, tx: &impl SchemaView) -> Result<Self> {
        let ast = parse_subscription(sql)?;
        let sub = type_subscription(ast, tx)?;
        let plan = compile_sub(sub);
        let plan = plan.optimize();
        Ok(Self { plan })
    }

    pub fn execute_bsatn(&self, tx: &impl Datastore) -> Result<(BsatnRowList, u64)> {
        execute_plan(&self.plan, tx, |iter| match iter {
            PlanIter::Index(iter) => BsatnFormat::encode_list(iter),
            PlanIter::Table(iter) => BsatnFormat::encode_list(iter),
            PlanIter::RowId(iter) => BsatnFormat::encode_list(iter),
            PlanIter::Tuple(iter) => BsatnFormat::encode_list(iter),
        })
    }
}
