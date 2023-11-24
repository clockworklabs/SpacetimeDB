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

/// Represents a logical database.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    /// The unique identity of the database.
    pub id: u64,
    /// The revision of the database.
    ///
    /// Incremented whenever the database is updated -- either by updating the
    /// associated stdb module, or by clearing its state while preserving the
    /// `address`.
    ///
    /// See also: [`DatabaseInstance::database_rev`]
    pub rev: u64,
    /// The stable address of the database.
    pub address: Address,
    /// The owner of the database, usually the publisher.
    pub identity: Identity,
    /// The runtime type of the associated stdb module.
    pub host_type: HostType,
    /// The desired number of replica [`DatabaseInstance`]s.
    pub num_replicas: u32,
    /// Content address of the associated stdb module.
    pub program_bytes_address: Hash,
    /// The client address used when publishing the database.
    pub publisher_address: Option<Address>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseStatus {
    pub state: String,
}

/// Represents a scheduled instance of a [`Database`].
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInstance {
    /// The unique identity of the instance.
    pub id: u64,
    /// The `id` of the logical [`Database`] this instance represents.
    pub database_id: u64,
    /// The revision of the logical [`Database`] this instance corresponds to.
    ///
    /// Note that, at any point in time, [`DatabaseInstance`]s representing
    /// different revisions may remain scheduled, possibly in different
    /// lifecycle states. This field thus allows to determine which revision the
    /// instance represents, even if the revision of the corresponding database
    /// has changed meanwhile.
    ///
    /// See also: [`Database::rev`].
    pub database_rev: u64,
    /// The [`Node`] this instance is scheduled on.
    ///
    /// Note that this represents the _desired state_, the [`Node`] may not yet
    /// have scheduled the instance.
    pub node_id: u64,
    /// When there are multiple replicas of `(database_id, database_rev)`, this
    /// instance is the designated leader.
    pub leader: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInstanceStatus {
    pub state: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    pub unschedulable: bool,
    /// TODO: It's unclear if this should be in here since it's arguably status
    /// rather than part of the configuration kind of. I dunno.
    pub advertise_addr: String,
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
    Wasmtime = 0,
}
