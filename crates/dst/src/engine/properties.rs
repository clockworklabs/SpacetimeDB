use super::workload::{Interaction, Model, Observation, TableSummary};
use crate::schema::SchemaPlan;
use crate::traits::Properties;

pub struct EngineProperties {
    oracle: EngineOracle,
    count_visible: CountVisible,
    commit_matches: CommitMatches,
    replay_matches: ReplayMatches,
}

impl EngineProperties {
    pub fn new(schema: SchemaPlan) -> Self {
        Self {
            oracle: EngineOracle::new(schema),
            count_visible: CountVisible,
            commit_matches: CommitMatches,
            replay_matches: ReplayMatches,
        }
    }
}

impl Properties<Interaction, Observation> for EngineProperties {
    fn observe(&mut self, interaction: &Interaction, observation: &Observation) -> Result<(), anyhow::Error> {
        self.oracle.apply(interaction);
        self.count_visible.check(interaction, observation, &self.oracle)?;
        self.commit_matches.check(interaction, observation, &self.oracle)?;
        self.replay_matches.check(interaction, observation, &self.oracle)?;
        Ok(())
    }
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

    fn apply(&mut self, interaction: &Interaction) {
        self.model.apply(interaction);
    }

    fn row_count(&self, table: usize) -> u64 {
        self.model.row_count(table)
    }

    fn summaries(&self) -> Vec<TableSummary> {
        self.model.summaries()
    }
}

struct CountVisible;

impl CountVisible {
    fn check(&self, interaction: &Interaction, observation: &Observation, oracle: &EngineOracle) -> anyhow::Result<()> {
        let Interaction::Count { table } = interaction else {
            return Ok(());
        };
        let Observation::Counted { count } = observation else {
            anyhow::bail!("count_visible: count produced unexpected observation");
        };

        anyhow::ensure!(
            *count == oracle.row_count(*table),
            "count_visible: count did not reflect visible transaction state for table {table}"
        );
        Ok(())
    }
}

struct CommitMatches;

impl CommitMatches {
    fn check(&self, interaction: &Interaction, observation: &Observation, oracle: &EngineOracle) -> anyhow::Result<()> {
        if !matches!(interaction, Interaction::CommitTx) {
            return Ok(());
        }
        let Observation::Committed { summaries } = observation else {
            anyhow::bail!("commit_matches: commit produced unexpected observation");
        };

        anyhow::ensure!(
            summaries == &oracle.summaries(),
            "commit_matches: committed target summary diverged from model"
        );
        Ok(())
    }
}

struct ReplayMatches;

impl ReplayMatches {
    fn check(&self, interaction: &Interaction, observation: &Observation, oracle: &EngineOracle) -> anyhow::Result<()> {
        if !matches!(interaction, Interaction::Replay) {
            return Ok(());
        }
        let Observation::Replayed { summaries } = observation else {
            anyhow::bail!("replay_matches: replay produced unexpected observation");
        };

        anyhow::ensure!(
            summaries == &oracle.summaries(),
            "replay_matches: replayed target summary diverged from committed model"
        );
        Ok(())
    }
}
