use spacetimedb_lib::Identity;
use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::ser::Serialize;

use crate::address::Address;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentityEmail {
    pub identity: Identity,
    pub email: String,
}
/// An energy balance (per identity).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyBalance {
    pub identity: Identity,
    /// The balance for this identity this identity.
    /// NOTE: This is a signed integer, because it is possible
    /// for a user's balance to go negative. This is allowable
    /// for reasons of eventual consistency motivated by performance.
    pub balance: i128,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    pub id: u64,
    pub address: Address,
    pub identity: Identity,
    pub host_type: HostType,
    pub num_replicas: u32,
    pub program_bytes_address: Hash,
    pub publisher_address: Option<Address>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseStatus {
    pub state: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInstance {
    pub id: u64,
    pub database_id: u64,
    pub node_id: u64,
    pub leader: bool,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInstanceStatus {
    pub state: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    /// If `true`, no new user databases will be scheduled on this node.
    pub unschedulable: bool,
    /// The hostname this node is reachable at.
    ///
    /// If `None`, the node is not currently live.
    pub advertise_addr: Option<String>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeStatus {
    /// TODO: node memory, CPU, and storage capacity
    /// TODO: node memory, CPU, and storage allocatable capacity
    /// SEE: <https://kubernetes.io/docs/reference/kubernetes-api/cluster-resources/node-v1/#NodeStatus>
    pub state: String,
}
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, strum::EnumString, strum::AsRefStr,
)]
#[strum(serialize_all = "lowercase")]
#[repr(i32)]
pub enum HostType {
    Wasm = 0,
}
