use super::model::Model;
use super::workload::{Interaction, Observation};
use crate::schema::SchemaPlan;
use crate::traits::Properties;

pub struct EngineProperties {
    oracle: EngineOracle,
    properties: Vec<Box<dyn EngineProperty>>,
}

impl EngineProperties {
    pub fn new(schema: SchemaPlan) -> Self {
        let ignored_tables: Vec<usize> = schema
            .tables
            .iter()
            .enumerate()
            .filter_map(|(table, plan)| (!plan.sequences.is_empty()).then_some(table))
            .collect();
        Self {
            oracle: EngineOracle::new(schema),
            properties: vec![
                Box::new(CommitMatches {
                    ignored_tables: ignored_tables.clone(),
                }),
                Box::new(ReplayMatchesModel { ignored_tables }),
            ],
        }
    }
}

impl Properties<Interaction, Observation> for EngineProperties {
    fn observe(&mut self, interaction: &Interaction, observation: &Observation) -> Result<(), anyhow::Error> {
        let expected = self.oracle.apply(interaction);

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

    fn apply(&mut self, interaction: &Interaction) -> Observation {
        self.model.apply(interaction)
    }
}

struct CommitMatches {
    ignored_tables: Vec<usize>,
}

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

        anyhow::ensure!(
            filter_commit_delta(delta, &self.ignored_tables) == filter_commit_delta(expected, &self.ignored_tables),
            "commit_matches: committed delta diverged from model"
        );
        Ok(())
    }
}

struct ReplayMatchesModel {
    ignored_tables: Vec<usize>,
}

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
            filter_count_state(state, &self.ignored_tables) == filter_count_state(expected, &self.ignored_tables),
            "replay_matches_model: replayed state diverged from model"
        );
        Ok(())
    }
}

fn filter_commit_delta(delta: &super::workload::CommitDelta, ignored_tables: &[usize]) -> super::workload::CommitDelta {
    super::workload::CommitDelta {
        tables: delta
            .tables
            .iter()
            .filter(|table| !ignored_tables.contains(&table.table))
            .cloned()
            .collect(),
    }
}

fn filter_count_state(state: &super::workload::CountState, ignored_tables: &[usize]) -> super::workload::CountState {
    super::workload::CountState {
        row_counts: state
            .row_counts
            .iter()
            .filter(|table| !ignored_tables.contains(&table.table))
            .copied()
            .collect(),
    }
}
