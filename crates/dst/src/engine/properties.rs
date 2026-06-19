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
        Self {
            oracle: EngineOracle::new(schema),
            properties: vec![Box::new(CountVisible), Box::new(CommitMatches), Box::new(ReplayMatches)],
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

struct CountVisible;

impl EngineProperty for CountVisible {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::Count { .. })
    }

    fn check(
        &self,
        _interaction: &Interaction,
        observation: &Observation,
        expected: &Observation,
    ) -> anyhow::Result<()> {
        let Observation::Counted { count } = observation else {
            anyhow::bail!("count_visible: count produced unexpected observation");
        };
        let Observation::Counted { count: expected } = expected else {
            unreachable!("CountVisible only subscribes to count interactions");
        };

        anyhow::ensure!(
            count == expected,
            "count_visible: count did not reflect visible transaction state"
        );
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
        let Observation::Committed { delta } = observation else {
            anyhow::bail!("commit_matches: commit produced unexpected observation");
        };
        let Observation::Committed { delta: expected } = expected else {
            unreachable!("CommitMatches only subscribes to commit interactions");
        };

        anyhow::ensure!(delta == expected, "commit_matches: committed delta diverged from model");
        Ok(())
    }
}

struct ReplayMatches;

impl EngineProperty for ReplayMatches {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::Replay)
    }

    fn check(
        &self,
        _interaction: &Interaction,
        observation: &Observation,
        expected: &Observation,
    ) -> anyhow::Result<()> {
        let Observation::Replayed { summaries } = observation else {
            anyhow::bail!("replay_matches: replay produced unexpected observation");
        };
        let Observation::Replayed { summaries: expected } = expected else {
            unreachable!("ReplayMatches only subscribes to replay interactions");
        };

        anyhow::ensure!(
            summaries == expected,
            "replay_matches: replayed target summary diverged from committed model"
        );
        Ok(())
    }
}
