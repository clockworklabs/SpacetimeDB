use std::cell::Cell;

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
        let auto_inc_table = schema.auto_inc_table_and_column().map(|(table, _)| table);
        Self {
            oracle: EngineOracle::new(schema),
            properties: vec![
                Box::new(CountVisible),
                Box::new(CommitMatches {
                    ignored_table: auto_inc_table,
                }),
                Box::new(AutoIncIncreasing::default()),
                Box::new(ReplayMatches {
                    ignored_table: auto_inc_table,
                }),
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

struct CommitMatches {
    ignored_table: Option<usize>,
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
            filter_commit_delta(delta, self.ignored_table) == filter_commit_delta(expected, self.ignored_table),
            "commit_matches: committed delta diverged from model"
        );
        Ok(())
    }
}

struct AutoIncIncreasing {
    max_seen: Cell<Option<u64>>,
}

impl Default for AutoIncIncreasing {
    fn default() -> Self {
        Self {
            max_seen: Cell::new(None),
        }
    }
}

impl EngineProperty for AutoIncIncreasing {
    fn observes(&self, interaction: &Interaction) -> bool {
        matches!(interaction, Interaction::CommitTx)
    }

    fn check(
        &self,
        _interaction: &Interaction,
        observation: &Observation,
        _expected: &Observation,
    ) -> anyhow::Result<()> {
        let Observation::Committed { auto_inc_values, .. } = observation else {
            anyhow::bail!("auto_inc_increasing: commit produced unexpected observation");
        };

        let mut previous = self.max_seen.get().unwrap_or(0);
        for &value in auto_inc_values {
            anyhow::ensure!(
                value > previous,
                "auto_inc_increasing: observed value {value} after {previous}"
            );
            previous = value;
        }
        self.max_seen.set(Some(previous));

        Ok(())
    }
}

struct ReplayMatches {
    ignored_table: Option<usize>,
}

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
            filter_summaries(summaries, self.ignored_table) == filter_summaries(expected, self.ignored_table),
            "replay_matches: replayed target summary diverged from committed model"
        );
        Ok(())
    }
}

fn filter_commit_delta(
    delta: &super::workload::CommitDelta,
    ignored_table: Option<usize>,
) -> super::workload::CommitDelta {
    let Some(ignored_table) = ignored_table else {
        return delta.clone();
    };

    super::workload::CommitDelta {
        tables: delta
            .tables
            .iter()
            .filter(|table| table.table != ignored_table)
            .cloned()
            .collect(),
    }
}

fn filter_summaries(
    summaries: &[super::workload::TableSummary],
    ignored_table: Option<usize>,
) -> Vec<super::workload::TableSummary> {
    summaries
        .iter()
        .enumerate()
        .filter(|(table, _)| Some(*table) != ignored_table)
        .map(|(_, summary)| *summary)
        .collect()
}
