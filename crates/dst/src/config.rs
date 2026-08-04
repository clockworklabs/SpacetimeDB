use crate::engine::EngineConfig;

#[derive(Clone, Debug, PartialEq)]
pub struct RunConfig {
    pub seed: u64,
    pub max_interactions: usize,
    pub target: RunTargetConfig,
}

impl RunConfig {
    pub fn engine(seed: u64, max_interactions: usize) -> Self {
        Self {
            seed,
            max_interactions,
            target: RunTargetConfig::Engine(EngineConfig::default()),
        }
    }

    pub fn replication(seed: u64, max_interactions: usize) -> Self {
        Self {
            seed,
            max_interactions,
            target: RunTargetConfig::Replication(ReplicationConfig::default()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunTargetConfig {
    Engine(EngineConfig),
    Replication(ReplicationConfig),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplicationConfig {}
