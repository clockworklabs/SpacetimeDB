use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::ser::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicaState {
    pub replica_id: u64,
    pub initialized: bool,
}
