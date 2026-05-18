mod random_crud;

use crate::{client::SessionId, schema::SchemaPlan, sim::Rng};

use super::{generation::ScenarioPlanner, TableScenario, TableWorkloadOutcome};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RandomCrudScenario;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TableScenarioId {
    #[default]
    RandomCrud,
}

impl TableScenario for RandomCrudScenario {
    fn generate_schema(&self, rng: &Rng) -> SchemaPlan {
        random_crud::generate_schema(rng)
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        random_crud::validate_outcome(schema, outcome)
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: SessionId) {
        random_crud::fill_pending(planner, conn);
    }
}

impl TableScenario for TableScenarioId {
    fn generate_schema(&self, rng: &Rng) -> SchemaPlan {
        match self {
            Self::RandomCrud => RandomCrudScenario.generate_schema(rng),
        }
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        match self {
            Self::RandomCrud => RandomCrudScenario.validate_outcome(schema, outcome),
        }
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: SessionId) {
        match self {
            Self::RandomCrud => RandomCrudScenario.fill_pending(planner, conn),
        }
    }
}
