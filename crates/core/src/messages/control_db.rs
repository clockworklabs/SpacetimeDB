use spacetimedb_lib::Identity;
use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::ser::Serialize;

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

/// Description of a database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    /// Internal id of the database, assigned by the control database.
    pub id: u64,
    /// Public identity (i.e. [`Identity`]) of the database.
    pub database_identity: Identity,
    /// [`Identity`] of the database's owner.
    pub owner_identity: Identity,
    /// [`HostType`] of the module associated with the database.
    ///
    /// Valid only for as long as `initial_program` is valid.
    pub host_type: HostType,
    /// [`Hash`] of the compiled module to initialize the database with.
    ///
    /// Updating the database's module will **not** change this value.
    pub initial_program: Hash,
}

impl Database {
    pub fn for_testing() -> Self {
        Self {
            id: 0,
            database_identity: Default::default(),
            owner_identity: Default::default(),
            host_type: HostType::Wasm,
            initial_program: Hash::ZERO,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseStatus {
    pub state: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Replica {
    pub id: u64,
    pub database_id: u64,
    pub node_id: u64,
    pub leader: bool,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicaStatus {
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
