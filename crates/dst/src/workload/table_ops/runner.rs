use crate::{
    config::RunConfig,
    core::{self, PropertySet},
    schema::SchemaPlan,
    seed::DstSeed,
};

use super::{
    model::ExpectedModel, NextInteractionGenerator, TableScenario, TableWorkloadEngine, TableWorkloadInteraction,
    TableWorkloadOutcome,
};

struct TablePropertyRuntime<S> {
    scenario: S,
    schema: SchemaPlan,
    expected: ExpectedModel,
}

impl<S: TableScenario> TablePropertyRuntime<S> {
    fn new(scenario: S, schema: SchemaPlan, num_connections: usize) -> Self {
        let table_count = schema.tables.len();
        Self {
            scenario,
            schema,
            expected: ExpectedModel::new(table_count, num_connections),
        }
    }
}

impl<S: TableScenario> PropertySet<TableWorkloadInteraction, TableWorkloadOutcome> for TablePropertyRuntime<S> {
    type Error = String;

    fn on_interaction(&mut self, interaction: &TableWorkloadInteraction, _step: usize) -> Result<(), Self::Error> {
        self.expected.apply(interaction);
        Ok(())
    }

    fn on_finish(&mut self, outcome: &TableWorkloadOutcome) -> Result<(), Self::Error> {
        let expected_rows = self.expected.clone().committed_rows();
        if outcome.final_rows != expected_rows {
            return Err(format!(
                "final datastore state mismatch: expected={expected_rows:?} actual={:?}",
                outcome.final_rows
            ));
        }
        self.scenario
            .validate_outcome(&self.schema, outcome)
            .map_err(|err| format!("scenario invariant failed: {err}"))
    }
}

pub fn run_generated_with_engine<S, E>(
    seed: DstSeed,
    scenario: S,
    config: RunConfig,
    make_engine: impl FnOnce(&SchemaPlan, usize) -> anyhow::Result<E>,
) -> anyhow::Result<TableWorkloadOutcome>
where
    S: TableScenario,
    E: TableWorkloadEngine,
{
    let mut rng = seed.fork(17).rng();
    let num_connections = rng.index(3) + 1;
    let schema = scenario.generate_schema(&mut rng);
    let generator = NextInteractionGenerator::new(
        seed,
        scenario.clone(),
        schema.clone(),
        num_connections,
        config.max_interactions_or_default(usize::MAX),
    );
    let engine = make_engine(&schema, num_connections)?;
    let properties = TablePropertyRuntime::new(scenario, schema, num_connections);
    core::run_streaming(generator, engine, properties, config)
}
