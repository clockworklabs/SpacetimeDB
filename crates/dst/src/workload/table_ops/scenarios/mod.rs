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

impl TableScenarioId {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "random-crud" => Ok(Self::RandomCrud),
            _ => anyhow::bail!("unsupported scenario: {value}; expected: random-crud"),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RandomCrud => "random-crud",
        }
    }
}

impl std::fmt::Display for TableScenarioId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
