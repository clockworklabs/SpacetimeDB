use std::path::Path;

use crate::{
    bugbase::{load_json, save_json},
    config::RunConfig,
    schema::SchemaPlan,
    seed::DstSeed,
    shrink::shrink_by_removing,
    subsystem::RunRecord,
    trace::Trace,
    workload::table_ops::{
        default_target_ops, execute_interactions, run_generated_with_engine, InteractionStream, TableScenario,
        TableScenarioId, TableWorkloadCase, TableWorkloadEngine, TableWorkloadEvent, TableWorkloadExecutionFailure,
        TableWorkloadOutcome,
    },
};

pub trait TableTargetHarness {
    type Engine: TableWorkloadEngine;

    fn target_name() -> &'static str;
    fn connection_seed_discriminator() -> u64;
    fn build_engine(schema: &SchemaPlan, num_connections: usize) -> anyhow::Result<Self::Engine>;

    fn can_remove_interaction(interaction: &crate::workload::table_ops::TableWorkloadInteraction) -> bool {
        !matches!(
            interaction,
            crate::workload::table_ops::TableWorkloadInteraction::CommitTx { .. }
                | crate::workload::table_ops::TableWorkloadInteraction::RollbackTx { .. }
        )
    }
}

pub fn materialize_case<T: TableTargetHarness>(
    seed: DstSeed,
    scenario: TableScenarioId,
    max_interactions: usize,
) -> TableWorkloadCase {
    let mut rng = seed.fork(T::connection_seed_discriminator()).rng();
    let num_connections = rng.index(3) + 1;
    let schema = scenario.generate_schema(&mut rng);
    let interactions =
        InteractionStream::new(seed, scenario, schema.clone(), num_connections, max_interactions).collect();
    TableWorkloadCase {
        seed,
        scenario,
        num_connections,
        schema,
        interactions,
    }
}

pub fn generate_case<T: TableTargetHarness>(seed: DstSeed, scenario: TableScenarioId) -> TableWorkloadCase {
    let mut rng = seed.fork(T::connection_seed_discriminator()).rng();
    materialize_case::<T>(seed, scenario, default_target_ops(&mut rng))
}

pub fn run_case_detailed<T: TableTargetHarness>(
    case: &TableWorkloadCase,
) -> Result<RunRecord<TableWorkloadCase, TableWorkloadEvent, TableWorkloadOutcome>, TableWorkloadExecutionFailure> {
    let mut trace = Trace::default();
    for interaction in &case.interactions {
        trace.push(TableWorkloadEvent::Executed(interaction.clone()));
    }

    let outcome = execute_interactions(
        &case.scenario,
        &case.schema,
        case.num_connections,
        case.interactions.clone(),
        T::build_engine,
    )?;

    Ok(RunRecord {
        subsystem: T::target_name(),
        seed: case.seed,
        case: case.clone(),
        trace: Some(trace),
        outcome,
    })
}

pub fn run_generated_with_config_and_scenario<T: TableTargetHarness>(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<TableWorkloadOutcome> {
    run_generated_with_engine(seed, scenario, config, T::build_engine)
}

pub fn save_case(path: impl AsRef<Path>, case: &TableWorkloadCase) -> anyhow::Result<()> {
    save_json(path, case)
}

pub fn load_case(path: impl AsRef<Path>) -> anyhow::Result<TableWorkloadCase> {
    load_json(path)
}

pub fn failure_reason<T: TableTargetHarness>(case: &TableWorkloadCase) -> anyhow::Result<String> {
    match run_case_detailed::<T>(case) {
        Ok(_) => anyhow::bail!("case did not fail"),
        Err(failure) => Ok(failure.reason),
    }
}

pub fn shrink_failure<T: TableTargetHarness>(
    case: &TableWorkloadCase,
    failure: &TableWorkloadExecutionFailure,
) -> anyhow::Result<TableWorkloadCase> {
    shrink_by_removing(
        case,
        failure,
        |case| {
            let mut shrunk = case.clone();
            shrunk.interactions.truncate(failure.step_index.saturating_add(1));
            shrunk
        },
        |case| case.interactions.len(),
        |case, idx| {
            let interaction = case.interactions.get(idx)?;
            if !T::can_remove_interaction(interaction) {
                return None;
            }
            let mut interactions = case.interactions.clone();
            interactions.remove(idx);
            Some(TableWorkloadCase {
                seed: case.seed,
                scenario: case.scenario,
                num_connections: case.num_connections,
                schema: case.schema.clone(),
                interactions,
            })
        },
        |case| match run_case_detailed::<T>(case) {
            Ok(_) => anyhow::bail!("case did not fail"),
            Err(failure) => Ok(failure),
        },
        |expected, candidate| expected.reason == candidate.reason,
    )
}
