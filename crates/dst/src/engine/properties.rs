use super::model::Model;
use super::row::Row;
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
            (
                Interaction::Insert { table, .. },
                Observation::Inserted {
                    outcome: InsertOutcome::Accepted(row),
                },
            ) => self.apply_insert(*table, row),
            (
                Interaction::Insert { .. },
                Observation::Inserted {
                    outcome: InsertOutcome::UniqueConstraintViolation,
                },
            ) => self.model.apply(interaction),
            (Interaction::Insert { .. }, _) => anyhow::bail!("insert produced unexpected observation"),
            _ => self.model.apply(interaction),
        };

        Ok(observation)
    }

    fn apply_insert(&mut self, table: usize, row: &Row) -> Observation {
        self.model.apply(&Interaction::Insert {
            table,
            row: row.clone(),
        })
    }
}

struct InsertMatches;

impl EngineProperty for InsertMatches {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::Insert { .. })
    }

    fn check(
        &self,
        _interaction: &Interaction,
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
            (InsertOutcome::UniqueConstraintViolation, InsertOutcome::UniqueConstraintViolation) => {}
            (InsertOutcome::Accepted(_), InsertOutcome::UniqueConstraintViolation) => {
                anyhow::bail!("insert_matches: target accepted row rejected by model");
            }
            (InsertOutcome::UniqueConstraintViolation, InsertOutcome::Accepted(_)) => {
                anyhow::bail!("insert_matches: target rejected row accepted by model");
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
            "replay_matches_model: replayed state diverged from model"
        );
        Ok(())
    }
}
