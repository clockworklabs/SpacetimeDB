use crate::dml::MutationPlan;
use crate::plan::ProjectListPlan;
use anyhow::Result;
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::identity::AuthCtx;
use std::collections::HashMap;

pub mod compile;
pub mod dml;
pub mod plan;
pub mod printer;
pub mod rules;

#[derive(Debug)]
pub enum PlanCtx {
    ProjectList(ProjectListPlan),
    DML(MutationPlan),
}

impl PlanCtx {
    pub(crate) fn optimize(self, auth: &AuthCtx) -> Result<PlanCtx> {
        Ok(match self {
            Self::ProjectList(plan) => Self::ProjectList(plan.optimize(auth)?),
            Self::DML(plan) => Self::DML(plan.optimize(auth)?),
        })
    }
}

/// A physical context for the result of a query compilation.
pub struct PhysicalCtx<'a> {
    pub plan: PlanCtx,
    pub auth: &'a AuthCtx,
    pub sql: &'a str,
    // A map from table names to their labels
    pub vars: HashMap<String, usize>,
    pub source: StatementSource,
    pub planning_time: Option<std::time::Duration>,
}

impl PhysicalCtx<'_> {
    pub fn optimize(self) -> Result<Self> {
        Ok(Self {
            plan: self.plan.optimize(self.auth)?,
            ..self
        })
    }
}
