use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::engine::EngineConfig;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RunTargetConfig {
    Engine(EngineConfig),
    Replication(ReplicationConfig),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub cluster: ReplicationClusterConfig,
    pub workload: ReplicationWorkloadConfig,
    pub faults: ReplicationFaultConfig,
    pub properties: ReplicationPropertyConfig,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            cluster: ReplicationClusterConfig::default(),
            workload: ReplicationWorkloadConfig::default(),
            faults: ReplicationFaultConfig::default(),
            properties: ReplicationPropertyConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationClusterConfig {
    pub replica_counts: Vec<usize>,
    pub replica_id_base: ReplicationReplicaIdBase,
}

impl Default for ReplicationClusterConfig {
    fn default() -> Self {
        Self {
            replica_counts: vec![3, 5, 7],
            replica_id_base: ReplicationReplicaIdBase::Random,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationReplicaIdBase {
    Random,
    Fixed(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationWorkloadConfig {
    pub client_count: usize,
    pub max_bytes_len: usize,
    pub max_string_len: usize,
    pub leader_poll_interval: Option<Duration>,
    pub max_appends_between_leader_polls: usize,
}

impl Default for ReplicationWorkloadConfig {
    fn default() -> Self {
        Self {
            client_count: 10,
            max_bytes_len: 256,
            max_string_len: 128,
            leader_poll_interval: None,
            max_appends_between_leader_polls: 100,
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationFaultConfig {
    pub no_fault: u64,
    pub partition: u64,
    pub one_way_link: u64,
    pub heal_network: u64,
    pub freeze_node: u64,
    pub unfreeze_node: u64,
}

impl Default for ReplicationFaultConfig {
    fn default() -> Self {
        Self {
            no_fault: 250,
            partition: 5,
            one_way_link: 5,
            heal_network: 5,
            freeze_node: 2,
            unfreeze_node: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationPropertyConfig {
    pub cluster_availability_timeout_polls: u32,
}

impl Default for ReplicationPropertyConfig {
    fn default() -> Self {
        Self {
            cluster_availability_timeout_polls: 10,
        }
    }
}
