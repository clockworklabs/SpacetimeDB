mod banking;
mod random_crud;

use serde::{Deserialize, Serialize};

use crate::{schema::SchemaPlan, seed::DstRng};

use super::{
    generation::ScenarioPlanner, TableProperty, TableScenario, TableWorkloadInteraction, TableWorkloadOutcome,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RandomCrudScenario;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct IndexedRangesScenario;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BankingScenario;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TableScenarioId {
    #[default]
    RandomCrud,
    IndexedRanges,
    Banking,
}

impl TableScenario for RandomCrudScenario {
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

impl TableScenario for IndexedRangesScenario {
    fn generate_schema(&self, rng: &mut DstRng) -> SchemaPlan {
        random_crud::generate_indexed_ranges_schema(rng)
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        random_crud::validate_outcome(schema, outcome)
    }

    fn commit_properties(&self) -> Vec<TableWorkloadInteraction> {
        Vec::new()
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize) {
        random_crud::fill_pending_indexed_ranges(planner, conn);
    }
}

impl TableScenario for TableScenarioId {
    fn generate_schema(&self, rng: &mut DstRng) -> SchemaPlan {
        match self {
            Self::RandomCrud => RandomCrudScenario.generate_schema(rng),
            Self::IndexedRanges => IndexedRangesScenario.generate_schema(rng),
            Self::Banking => BankingScenario.generate_schema(rng),
        }
    }

    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()> {
        match self {
            Self::RandomCrud => RandomCrudScenario.validate_outcome(schema, outcome),
            Self::IndexedRanges => IndexedRangesScenario.validate_outcome(schema, outcome),
            Self::Banking => BankingScenario.validate_outcome(schema, outcome),
        }
    }

    fn commit_properties(&self) -> Vec<TableWorkloadInteraction> {
        match self {
            Self::RandomCrud => RandomCrudScenario.commit_properties(),
            Self::IndexedRanges => IndexedRangesScenario.commit_properties(),
            Self::Banking => BankingScenario.commit_properties(),
        }
    }

    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize) {
        match self {
            Self::RandomCrud => RandomCrudScenario.fill_pending(planner, conn),
            Self::IndexedRanges => IndexedRangesScenario.fill_pending(planner, conn),
            Self::Banking => BankingScenario.fill_pending(planner, conn),
        }
    }
}
