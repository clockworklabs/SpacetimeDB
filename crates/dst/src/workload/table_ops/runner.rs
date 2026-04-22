use std::time::Instant;

use crate::{config::RunConfig, schema::SchemaPlan, seed::DstSeed};

use super::{
    model::ExpectedModel, InteractionStream, TableScenario, TableWorkloadEngine, TableWorkloadExecutionFailure,
    TableWorkloadInteraction, TableWorkloadOutcome,
};

pub fn execute_interactions<S, E, I>(
    scenario: &S,
    schema: &SchemaPlan,
    num_connections: usize,
    interactions: I,
    make_engine: impl FnOnce(&SchemaPlan, usize) -> anyhow::Result<E>,
) -> Result<TableWorkloadOutcome, TableWorkloadExecutionFailure>
where
    S: TableScenario,
    E: TableWorkloadEngine,
    I: IntoIterator<Item = TableWorkloadInteraction>,
{
    let mut engine =
        make_engine(schema, num_connections).map_err(|err| failure_without_step(format!("bootstrap failed: {err}")))?;
    let mut expected = ExpectedModel::new(schema.tables.len(), num_connections);

    for (step_index, interaction) in interactions.into_iter().enumerate() {
        engine
            .execute(&interaction)
            .map_err(|reason| TableWorkloadExecutionFailure {
                step_index,
                reason,
                interaction: Some(interaction.clone()),
            })?;
        expected.apply(&interaction);
    }

    engine.finish();
    let outcome = engine
        .collect_outcome()
        .map_err(|err| failure_without_step(format!("collect outcome failed: {err}")))?;
    let expected_rows = expected.committed_rows();
    if outcome.final_rows != expected_rows {
        return Err(failure_without_step(format!(
            "final datastore state mismatch: expected={expected_rows:?} actual={:?}",
            outcome.final_rows
        )));
    }

    scenario
        .validate_outcome(schema, &outcome)
        .map_err(|err| failure_without_step(format!("scenario invariant failed: {err}")))?;

    Ok(outcome)
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
    let mut stream = InteractionStream::new(
        seed,
        scenario.clone(),
        schema.clone(),
        num_connections,
        config.max_interactions_or_default(usize::MAX),
    );
    let mut engine = make_engine(&schema, num_connections)?;
    let mut expected = ExpectedModel::new(schema.tables.len(), num_connections);
    let deadline = config.deadline();

    let mut step_index = 0usize;
    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            stream.request_finish();
        }

        let Some(interaction) = stream.next() else {
            break;
        };
        engine
            .execute(&interaction)
            .map_err(|reason| anyhow::anyhow!("workload failed at step {step_index}: {reason}"))?;
        expected.apply(&interaction);
        step_index = step_index.saturating_add(1);
    }

    engine.finish();
    let outcome = engine.collect_outcome()?;
    let expected_rows = expected.committed_rows();
    if outcome.final_rows != expected_rows {
        anyhow::bail!(
            "final datastore state mismatch: expected={expected_rows:?} actual={:?}",
            outcome.final_rows
        );
    }
    scenario.validate_outcome(&schema, &outcome)?;
    Ok(outcome)
}

fn failure_without_step(reason: String) -> TableWorkloadExecutionFailure {
    TableWorkloadExecutionFailure {
        step_index: usize::MAX,
        reason,
        interaction: None,
    }
}
