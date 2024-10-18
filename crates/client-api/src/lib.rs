use std::sync::Arc;

use async_trait::async_trait;
use axum::response::ErrorResponse;
use http::StatusCode;

use spacetimedb::address::Address;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
use spacetimedb::host::{HostController, UpdateDatabaseResult};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, HostType, Node, Replica};
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, Tld};

pub mod auth;
pub mod routes;
pub mod util;

/// Defines the state / environment of a SpacetimeDB node from the PoV of the
/// client API.
///
/// Types returned here should be considered internal state and **never** be
/// surfaced to the API.
pub trait NodeDelegate: Send + Sync {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily>;
    fn host_controller(&self) -> &HostController;
    fn client_actor_index(&self) -> &ClientActorIndex;

    /// Return a JWT decoding key for verifying credentials.
    fn public_key(&self) -> &DecodingKey;

    // The issuer to use when signing JWTs.
    fn local_issuer(&self) -> String;

    /// Return the public key used to verify JWTs, as the bytes of a PEM public key file.
    ///
    /// The `/identity/public-key` route calls this method to return the public key to callers.
    fn public_key_bytes(&self) -> &[u8];

    /// Return a JWT encoding key for signing credentials.
    fn private_key(&self) -> &EncodingKey;
}

/// Parameters for publishing a database.
///
/// See [`ControlStateDelegate::publish_database`].
pub struct DatabaseDef {
    /// The [`Identity`] the database shall have.
    ///
    /// Addresses are allocated via [`ControlStateDelegate::create_address`].
    pub database_identity: Identity,
    /// The compiled program of the database module.
    pub program_bytes: Vec<u8>,
    /// The desired number of replicas the database shall have.
    pub num_replicas: u32,
    /// The host type of the supplied program.
    pub host_type: HostType,
}

/// API of the SpacetimeDB control plane.
///
/// The trait is the composition of [`ControlStateReadAccess`] and
/// [`ControlStateWriteAccess`] to reflect the consistency model of SpacetimeDB
/// as of this writing:
///
/// The control plane state represents the _desired_ state of an ensemble of
/// SpacetimeDB nodes. As such, this state can be read from a local (in-memory)
/// representation, which is guaranteed to be "prefix consistent" across all
/// nodes of a cluster. Prefix consistency means that the state being examined
/// is consistent, but reads may not return the most recently written values.
///
/// As a consequence, implementations are not currently required to guarantee
/// read-after-write consistency. In the future, however, write operations may
/// be required to return the observed state after completing. As this may
/// require them to suspend themselves while waiting for the writes to propagate,
/// [`ControlStateWriteAccess`] methods are marked `async` today already.
#[async_trait]
pub trait ControlStateDelegate: ControlStateReadAccess + ControlStateWriteAccess + Send + Sync {}

impl<T: ControlStateReadAccess + ControlStateWriteAccess + Send + Sync> ControlStateDelegate for T {}

/// Query API of the SpacetimeDB control plane.
pub trait ControlStateReadAccess {
    // Nodes
    fn get_node_id(&self) -> Option<u64>;
    fn get_node_by_id(&self, node_id: u64) -> anyhow::Result<Option<Node>>;
    fn get_nodes(&self) -> anyhow::Result<Vec<Node>>;

    // Databases
    fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>>;
    fn get_database_by_identity(&self, database_identity: &Identity) -> anyhow::Result<Option<Database>>;
    fn get_databases(&self) -> anyhow::Result<Vec<Database>>;

    // Replicas
    fn get_replica_by_id(&self, id: u64) -> anyhow::Result<Option<Replica>>;
    fn get_replicas(&self) -> anyhow::Result<Vec<Replica>>;
    fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica>;

    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>>;

    // DNS
    fn lookup_identity(&self, domain: &DomainName) -> anyhow::Result<Option<Address>>;
    fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>>;
}

/// Write operations on the SpacetimeDB control plane.
#[async_trait]
pub trait ControlStateWriteAccess: Send + Sync {
    // Databases
    async fn create_address(&self) -> anyhow::Result<Address>;

    /// Publish a database acc. to [`DatabaseDef`].
    ///
    /// If the database with the given address was successfully published before,
    /// it is updated acc. to the module lifecycle conventions. `Some` result is
    /// returned in that case.
    ///
    /// Otherwise, `None` is returned meaning that the database was freshly
    /// initialized.
    async fn publish_database(
        &self,
        publisher: &Identity,
        spec: DatabaseDef,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>>;

    async fn delete_database(&self, identity: &Identity, address: &Address) -> anyhow::Result<()>;

    // Energy
    async fn add_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()>;
    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()>;

    // DNS
    async fn register_tld(&self, identity: &Identity, tld: Tld) -> anyhow::Result<RegisterTldResult>;
    async fn create_dns_record(
        &self,
        identity: &Identity,
        domain: &DomainName,
        database_identity: &Identity,
    ) -> anyhow::Result<InsertDomainResult>;
}

impl<T: ControlStateReadAccess + ?Sized> ControlStateReadAccess for Arc<T> {
    // Nodes
    fn get_node_id(&self) -> Option<u64> {
        (**self).get_node_id()
    }
    fn get_node_by_id(&self, node_id: u64) -> anyhow::Result<Option<Node>> {
        (**self).get_node_by_id(node_id)
    }
    fn get_nodes(&self) -> anyhow::Result<Vec<Node>> {
        (**self).get_nodes()
    }

    // Databases
    fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>> {
        (**self).get_database_by_id(id)
    }
    fn get_database_by_identity(&self, address: &Address) -> anyhow::Result<Option<Database>> {
        (**self).get_database_by_identity(address)
    }
    fn get_databases(&self) -> anyhow::Result<Vec<Database>> {
        (**self).get_databases()
    }

    // Replicas
    fn get_replica_by_id(&self, id: u64) -> anyhow::Result<Option<Replica>> {
        (**self).get_replica_by_id(id)
    }
    fn get_replicas(&self) -> anyhow::Result<Vec<Replica>> {
        (**self).get_replicas()
    }
    fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica> {
        (**self).get_leader_replica_by_database(database_id)
    }

    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
        (**self).get_energy_balance(identity)
    }

    // DNS
    fn lookup_identity(&self, domain: &DomainName) -> anyhow::Result<Option<Address>> {
        (**self).lookup_identity(domain)
    }

    fn reverse_lookup(&self, address: &Address) -> anyhow::Result<Vec<DomainName>> {
        (**self).reverse_lookup(address)
    }
}

#[async_trait]
impl<T: ControlStateWriteAccess + ?Sized> ControlStateWriteAccess for Arc<T> {
    async fn create_address(&self) -> anyhow::Result<Address> {
        (**self).create_address().await
    }
    
    async fn publish_database(
        &self,
        identity: &Identity,
        spec: DatabaseDef,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        (**self).publish_database(identity, spec).await
    }

    async fn delete_database(&self, identity: &Identity, address: &Address) -> anyhow::Result<()> {
        (**self).delete_database(identity, address).await
    }

    async fn add_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()> {
        (**self).add_energy(identity, amount).await
    }
    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()> {
        (**self).withdraw_energy(identity, amount).await
    }

    async fn register_tld(&self, identity: &Identity, tld: Tld) -> anyhow::Result<RegisterTldResult> {
        (**self).register_tld(identity, tld).await
    }

    async fn create_dns_record(
        &self,
        identity: &Identity,
        domain: &DomainName,
        address: &Address,
    ) -> anyhow::Result<InsertDomainResult> {
        (**self).create_dns_record(identity, domain, address).await
    }
}

impl<T: NodeDelegate + ?Sized> NodeDelegate for Arc<T> {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        (**self).gather_metrics()
    }

    fn local_issuer(&self) -> String {
        (**self).local_issuer()
    }

    fn host_controller(&self) -> &HostController {
        (**self).host_controller()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        (**self).client_actor_index()
    }

    fn public_key(&self) -> &DecodingKey {
        (**self).public_key()
    }

    fn public_key_bytes(&self) -> &[u8] {
        (**self).public_key_bytes()
    }

    fn private_key(&self) -> &EncodingKey {
        (**self).private_key()
    }
}

pub fn log_and_500(e: impl std::fmt::Display) -> ErrorResponse {
    log::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into()
}
