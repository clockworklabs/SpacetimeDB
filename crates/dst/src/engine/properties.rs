use super::model::Model;
use super::state::CountState;
use super::workload::{InsertOutcome, Interaction, Observation};
use crate::schema::SchemaPlan;
use crate::traits::Properties;

pub struct EngineProperties {
    oracle: EngineOracle,
    properties: Vec<Box<dyn EngineProperty>>,
}

impl EngineProperties {
    pub fn new(schema: SchemaPlan) -> Self {
        Self {
            oracle: EngineOracle::new(schema),
            properties: vec![
                Box::new(InsertMatches),
                Box::new(CommitMatches),
                Box::new(MigrateMatches),
                Box::new(ReplayMatchesModel),
            ],
        }
    }
}

impl Properties<Interaction, Observation> for EngineProperties {
    fn observe(&mut self, interaction: &Interaction, observation: &Observation) -> Result<(), anyhow::Error> {
        let expected = self.oracle.apply(interaction, observation)?;

        for property in &self.properties {
            if property.observes(interaction) {
                property.check(interaction, observation, &expected)?;
            }
        }

        Ok(())
    }
}

trait EngineProperty {
    fn observes(&self, interaction: &Interaction) -> bool;

    fn check(&self, interaction: &Interaction, observation: &Observation, expected: &Observation)
        -> anyhow::Result<()>;
}

struct EngineOracle {
    model: Model,
}

impl EngineOracle {
    fn new(schema: SchemaPlan) -> Self {
        Self {
            model: Model::new(schema),
        }
    }

    fn apply(&mut self, interaction: &Interaction, observation: &Observation) -> anyhow::Result<Observation> {
        let observation = match (interaction, observation) {
            (Interaction::Insert { .. }, Observation::Inserted { .. }) => self.model.apply(interaction),
            (Interaction::Insert { .. }, _) => anyhow::bail!("insert produced unexpected observation"),
            _ => self.model.apply(interaction),
        };

        Ok(observation)
    }
}

struct InsertMatches;

impl EngineProperty for InsertMatches {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::Insert { .. })
    }

    fn check(
        &self,
        interaction: &Interaction,
        observation: &Observation,
        expected: &Observation,
    ) -> anyhow::Result<()> {
        let Observation::Inserted { outcome } = observation else {
            anyhow::bail!("insert_matches: insert produced unexpected observation");
        };
        let Observation::Inserted { outcome: expected } = expected else {
            unreachable!("InsertMatches only subscribes to insert interactions");
        };

        match (outcome, expected) {
            (InsertOutcome::Accepted(row), InsertOutcome::Accepted(expected)) => {
                anyhow::ensure!(row == expected, "insert_matches: accepted row diverged from model");
            }
            (InsertOutcome::UniqueConstraintViolation { .. }, InsertOutcome::UniqueConstraintViolation { .. }) => {}
            (InsertOutcome::Accepted(_), InsertOutcome::UniqueConstraintViolation { .. }) => {
                anyhow::bail!(
                    "insert_matches: target accepted row rejected by model\ninteraction: {interaction:#?}\ntarget: {observation:#?}\nmodel: {expected:#?}"
                );
            }
            (InsertOutcome::UniqueConstraintViolation { .. }, InsertOutcome::Accepted(_)) => {
                anyhow::bail!(
                    "insert_matches: target rejected row accepted by model\ninteraction: {interaction:#?}\ntarget: {observation:#?}\nmodel: {expected:#?}"
                );
            }
        }

        Ok(())
    }
}

struct CommitMatches;

impl EngineProperty for CommitMatches {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::CommitTx)
    }

    fn check(
        &self,
        _interaction: &Interaction,
        observation: &Observation,
        expected: &Observation,
    ) -> anyhow::Result<()> {
        let Observation::Committed { delta, .. } = observation else {
            anyhow::bail!("commit_matches: commit produced unexpected observation");
        };
        let Observation::Committed { delta: expected, .. } = expected else {
            unreachable!("CommitMatches only subscribes to commit interactions");
        };

        anyhow::ensure!(delta == expected, "commit_matches: committed delta diverged from model");
        Ok(())
    }
}

struct MigrateMatches;

impl EngineProperty for MigrateMatches {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::Migrate(_))
    }

    fn check(
        &self,
        interaction: &Interaction,
        observation: &Observation,
        expected: &Observation,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            observation == expected,
            "migrate_matches: migration outcome diverged from model
interaction: {interaction:#?}
target: {observation:#?}
model: {expected:#?}"
        );
        Ok(())
    }
}

struct ReplayMatchesModel;

impl EngineProperty for ReplayMatchesModel {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::Replay)
    }

    fn check(
        &self,
        _interaction: &Interaction,
        observation: &Observation,
        expected: &Observation,
    ) -> anyhow::Result<()> {
        let Observation::Replayed { state } = observation else {
            anyhow::bail!("replay_matches_model: replay produced unexpected observation");
        };
        let Observation::Replayed { state: expected } = expected else {
            unreachable!("ReplayMatchesModel only subscribes to replay interactions");
        };

        anyhow::ensure!(
            state == expected,
            "replay_matches_model: replayed state diverged from model: {}",
            count_state_mismatch(state, expected)
        );
        Ok(())
    }
}

fn count_state_mismatch(target: &CountState, model: &CountState) -> String {
    if target.schema != model.schema {
        if target.schema.tables.len() != model.schema.tables.len() {
            return format!(
                "schema table count target={} model={}",
                target.schema.tables.len(),
                model.schema.tables.len()
            );
        }

        for (table_idx, (target_table, model_table)) in
            target.schema.tables.iter().zip(&model.schema.tables).enumerate()
        {
            if target_table == model_table {
                continue;
            }

            if target_table.name != model_table.name {
                return format!(
                    "schema table {table_idx} name target={:?} model={:?}",
                    target_table.name, model_table.name
                );
            }
            if target_table.columns != model_table.columns {
                return format!(
                    "schema table {table_idx} columns target={:?} model={:?}",
                    target_table.columns, model_table.columns
                );
            }
            if target_table.indexes != model_table.indexes {
                return format!(
                    "schema table {table_idx} indexes target={:?} model={:?}",
                    target_table.indexes, model_table.indexes
                );
            }
            if target_table.unique_constraints != model_table.unique_constraints {
                return format!(
                    "schema table {table_idx} unique constraints target={:?} model={:?}",
                    target_table.unique_constraints, model_table.unique_constraints
                );
            }
            if target_table.sequences != model_table.sequences {
                return format!(
                    "schema table {table_idx} sequences target={:?} model={:?}",
                    target_table.sequences, model_table.sequences
                );
            }
            return format!("schema table {table_idx} target={target_table:?} model={model_table:?}");
        }
    }

    if target.row_counts != model.row_counts {
        return format!("row counts target={:?} model={:?}", target.row_counts, model.row_counts);
    }

    if target.table_rows.len() != model.table_rows.len() {
        return format!(
            "table row set count target={} model={}",
            target.table_rows.len(),
            model.table_rows.len()
        );
    }

    for (table_idx, (target_rows, model_rows)) in target.table_rows.iter().zip(&model.table_rows).enumerate() {
        if target_rows == model_rows {
            continue;
        }
        if target_rows.table != model_rows.table {
            return format!(
                "table row entry {table_idx} id target={} model={}",
                target_rows.table, model_rows.table
            );
        }
        if target_rows.rows.len() != model_rows.rows.len() {
            return format!(
                "table {} row count target={} model={}",
                target_rows.table,
                target_rows.rows.len(),
                model_rows.rows.len()
            );
        }
        if let Some(row_idx) = target_rows
            .rows
            .iter()
            .zip(&model_rows.rows)
            .position(|(target_row, model_row)| target_row != model_row)
        {
            return format!(
                "table {} row {row_idx} target={:?} model={:?}",
                target_rows.table, target_rows.rows[row_idx], model_rows.rows[row_idx]
            );
        }
        return format!("table {} rows differ", target_rows.table);
    }

    "unknown mismatch".into()
}
