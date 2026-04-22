mod banking;
mod random_crud;

use serde::{Deserialize, Serialize};

use crate::{schema::SchemaPlan, seed::DstRng};

use super::{
    generation::ScenarioPlanner, TableProperty, TableScenario, TableWorkloadInteraction, TableWorkloadOutcome,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RandomCrudScenario;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BankingScenario;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TableScenarioId {
    #[default]
    RandomCrud,
    Banking,
}

impl TableScenario for RandomCrudScenario {
    fn name(&self) -> &'static str {
        "random-crud"
    }

    fn generate_schema(&self, rng: &mut DstRng) -> SchemaPlan {
        random_crud::generate_schema(rng)
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        random_crud::validate_outcome(schema, outcome)
    }

    fn commit_properties(&self) -> Vec<TableWorkloadInteraction> {
        Vec::new()
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize) {
        random_crud::fill_pending(planner, conn);
    }
}

impl TableScenario for BankingScenario {
    fn name(&self) -> &'static str {
        "banking"
    }

    fn generate_schema(&self, _rng: &mut DstRng) -> SchemaPlan {
        banking::generate_schema()
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        banking::validate_outcome(schema, outcome)
    }

    fn commit_properties(&self) -> Vec<TableWorkloadInteraction> {
        vec![super::properties::property_interaction(
            TableProperty::TablesMatchFresh { left: 0, right: 1 },
        )]
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize) {
        banking::fill_pending(planner, conn);
    }
}

impl TableScenario for TableScenarioId {
    fn name(&self) -> &'static str {
        match self {
            Self::RandomCrud => RandomCrudScenario.name(),
            Self::Banking => BankingScenario.name(),
        }
    }

    fn generate_schema(&self, rng: &mut DstRng) -> SchemaPlan {
        match self {
            Self::RandomCrud => RandomCrudScenario.generate_schema(rng),
            Self::Banking => BankingScenario.generate_schema(rng),
        }
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        match self {
            Self::RandomCrud => RandomCrudScenario.validate_outcome(schema, outcome),
            Self::Banking => BankingScenario.validate_outcome(schema, outcome),
        }
    }

    fn commit_properties(&self) -> Vec<TableWorkloadInteraction> {
        match self {
            Self::RandomCrud => RandomCrudScenario.commit_properties(),
            Self::Banking => BankingScenario.commit_properties(),
        }
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize) {
        match self {
            Self::RandomCrud => RandomCrudScenario.fill_pending(planner, conn),
            Self::Banking => BankingScenario.fill_pending(planner, conn),
        }
    }
}

pub fn default_target_ops(rng: &mut DstRng) -> usize {
    24 + rng.index(24)
}
