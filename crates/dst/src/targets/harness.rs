use crate::{
    config::RunConfig,
    schema::SchemaPlan,
    seed::DstSeed,
    workload::table_ops::{run_generated_with_engine, TableScenarioId, TableWorkloadEngine, TableWorkloadOutcome},
};

pub(crate) trait TableTargetHarness {
    type Engine: TableWorkloadEngine;

    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine>;
}

pub(crate) fn run_generated_with_config_and_scenario<T: TableTargetHarness>(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<TableWorkloadOutcome> {
    run_generated_with_engine(seed, scenario, config, T::build_engine)
}
