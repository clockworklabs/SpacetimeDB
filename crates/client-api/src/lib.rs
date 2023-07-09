use std::sync::Arc;

use async_trait::async_trait;
use http::StatusCode;

use spacetimedb::address::Address;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::host::HostController;
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, DatabaseInstance, EnergyBalance, IdentityEmail, Node};
use spacetimedb::messages::worker_db::DatabaseInstanceState;
use spacetimedb::module_host_context::ModuleHostContext;
use spacetimedb::sendgrid_controller::SendGridController;
use spacetimedb_lib::name::{DomainName, RegisterTldResult, Tld, InsertDomainResult};
use spacetimedb_lib::recovery::RecoveryCode;

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
    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController;
    fn host_controller(&self) -> &Arc<HostController>;
    fn client_actor_index(&self) -> &ClientActorIndex;
    fn public_key(&self) -> &DecodingKey;
    fn private_key(&self) -> &EncodingKey;
    fn sendgrid_controller(&self) -> Option<&SendGridController>;
}

/// Defines how to load a [`ModuleHostContext`].
#[async_trait]
pub trait ModuleHostContextLoader: Send + Sync {
    /// Load the [`ModuleHostContext`] for instance `instance_id` of
    /// [`Database`] `db`.
    ///
    /// This method is defined as `async`, as that obliges the implementer to
    /// ensure that any necessary blocking I/O is made async-safe. In other
    /// words, it is the responsibility of the implementation to make use of
    /// `spawn_blocking` or `block_in_place` as appropriate, while the
    /// `client-api` assumes that `await`ing the method never blocks.
    async fn load_module_host_context(&self, db: Database, instance_id: u64) -> anyhow::Result<ModuleHostContext>;
}

/// Parameters for publishing a database.
///
/// See [`ControlStateDelegate::publish_database`].
pub struct DatabaseDef {
    /// The [`Address`] the database shall have.
    ///
    /// Addresses are allocated via [`ControlStateDelegate::create_address`].
    pub address: Address,
    /// The compiled program of the database module.
    pub program_bytes: Vec<u8>,
    /// The desired number of replicas the database shall have.
    pub num_replicas: u32,
    /// It has been long forgotten what this does, if anything.
    pub trace_log: bool,
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
    fn get_node_by_id(&self, node_id: u64) -> spacetimedb::control_db::Result<Option<Node>>;
    fn get_nodes(&self) -> spacetimedb::control_db::Result<Vec<Node>>;

    // Databases
    fn get_database_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<Database>>;
    fn get_database_by_address(&self, address: &Address) -> spacetimedb::control_db::Result<Option<Database>>;
    fn get_databases(&self) -> spacetimedb::control_db::Result<Vec<Database>>;

    // Database instances
    fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> spacetimedb::control_db::Result<Option<DatabaseInstanceState>>;
    fn get_database_instance_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<DatabaseInstance>>;
    fn get_database_instances(&self) -> spacetimedb::control_db::Result<Vec<DatabaseInstance>>;
    fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance>;

    // Identities
    fn get_identities_for_email(&self, email: &str) -> spacetimedb::control_db::Result<Vec<IdentityEmail>>;
    fn get_recovery_codes(&self, email: &str) -> spacetimedb::control_db::Result<Vec<RecoveryCode>>;

    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> spacetimedb::control_db::Result<Option<EnergyBalance>>;

    // DNS
    fn lookup_address(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>>;
    fn reverse_lookup(&self, address: &Address) -> spacetimedb::control_db::Result<Vec<DomainName>>;
}

/// Write operations on the SpacetimeDB control plane.
#[async_trait]
pub trait ControlStateWriteAccess: Send + Sync {
    // Databases
    async fn create_address(&self) -> spacetimedb::control_db::Result<Address>;
    async fn publish_database(&self, identity: &Identity, spec: DatabaseDef) -> spacetimedb::control_db::Result<()>;
    async fn delete_database(&self, identity: &Identity, address: &Address) -> spacetimedb::control_db::Result<()>;

    // Identities
    async fn create_identity(&self) -> spacetimedb::control_db::Result<Identity>;
    async fn add_email(&self, identity: &Identity, email: &str) -> spacetimedb::control_db::Result<()>;
    async fn insert_recovery_code(
        &self,
        identity: &Identity,
        email: &str,
        code: RecoveryCode,
    ) -> spacetimedb::control_db::Result<()>;

    // Energy
    async fn add_energy(&self, identity: &Identity, quanta: u64) -> spacetimedb::control_db::Result<()>;

    // DNS
    async fn register_tld(&self, identity: &Identity, tld: Tld) -> spacetimedb::control_db::Result<RegisterTldResult>;
    async fn create_dns_record(
        &self,
        identity: &Identity,
        domain: &DomainName,
        address: &Address,
    ) -> spacetimedb::control_db::Result<InsertDomainResult>;
}

pub struct ArcEnv<T: ?Sized>(pub Arc<T>);
impl<T: ?Sized> Clone for ArcEnv<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: ControlStateReadAccess + ?Sized> ControlStateReadAccess for ArcEnv<T> {
    // Nodes
    fn get_node_id(&self) -> Option<u64> {
        self.0.get_node_id()
    }
    fn get_node_by_id(&self, node_id: u64) -> spacetimedb::control_db::Result<Option<Node>> {
        self.0.get_node_by_id(node_id)
    }
    fn get_nodes(&self) -> spacetimedb::control_db::Result<Vec<Node>> {
        self.0.get_nodes()
    }

    // Databases
    fn get_database_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<Database>> {
        self.0.get_database_by_id(id)
    }
    fn get_database_by_address(&self, address: &Address) -> spacetimedb::control_db::Result<Option<Database>> {
        self.0.get_database_by_address(address)
    }
    fn get_databases(&self) -> spacetimedb::control_db::Result<Vec<Database>> {
        self.0.get_databases()
    }

    // Database instances
    fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> spacetimedb::control_db::Result<Option<DatabaseInstanceState>> {
        self.0.get_database_instance_state(database_instance_id)
    }
    fn get_database_instance_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<DatabaseInstance>> {
        self.0.get_database_instance_by_id(id)
    }
    fn get_database_instances(&self) -> spacetimedb::control_db::Result<Vec<DatabaseInstance>> {
        self.0.get_database_instances()
    }
    fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.0.get_leader_database_instance_by_database(database_id)
    }

    // Identities
    fn get_identities_for_email(&self, email: &str) -> spacetimedb::control_db::Result<Vec<IdentityEmail>> {
        self.0.get_identities_for_email(email)
    }
    fn get_recovery_codes(&self, email: &str) -> spacetimedb::control_db::Result<Vec<RecoveryCode>> {
        self.0.get_recovery_codes(email)
    }

    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> spacetimedb::control_db::Result<Option<EnergyBalance>> {
        self.0.get_energy_balance(identity)
    }

    // DNS
    fn lookup_address(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>> {
        self.0.lookup_address(domain)
    }

    fn reverse_lookup(&self, address: &Address) -> spacetimedb::control_db::Result<Vec<DomainName>> {
        self.0.reverse_lookup(address)
    }
}

#[async_trait]
impl<T: ControlStateWriteAccess + ?Sized> ControlStateWriteAccess for ArcEnv<T> {
    async fn create_address(&self) -> spacetimedb::control_db::Result<Address> {
        self.0.create_address().await
    }

    async fn publish_database(&self, identity: &Identity, spec: DatabaseDef) -> spacetimedb::control_db::Result<()> {
        self.0.publish_database(identity, spec).await
    }
    async fn delete_database(&self, identity: &Identity, address: &Address) -> spacetimedb::control_db::Result<()> {
        self.0.delete_database(identity, address).await
    }

    async fn create_identity(&self) -> spacetimedb::control_db::Result<Identity> {
        self.0.create_identity().await
    }

    async fn add_email(&self, identity: &Identity, email: &str) -> spacetimedb::control_db::Result<()> {
        self.0.add_email(identity, email).await
    }

    async fn insert_recovery_code(
        &self,
        identity: &Identity,
        email: &str,
        code: RecoveryCode,
    ) -> spacetimedb::control_db::Result<()> {
        self.0.insert_recovery_code(identity, email, code).await
    }

    async fn add_energy(&self, identity: &Identity, quanta: u64) -> spacetimedb::control_db::Result<()> {
        self.0.add_energy(identity, quanta).await
    }

    async fn register_tld(&self, identity: &Identity, tld: Tld) -> spacetimedb::control_db::Result<RegisterTldResult> {
        self.0.register_tld(identity, tld).await
    }

    async fn create_dns_record(
        &self,
        identity: &Identity,
        domain: &DomainName,
        address: &Address,
    ) -> spacetimedb::control_db::Result<InsertDomainResult> {
        self.0.create_dns_record(identity, domain, address).await
    }
}

impl<T: NodeDelegate + ?Sized> NodeDelegate for ArcEnv<T> {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.0.gather_metrics()
    }

    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController {
        self.0.database_instance_context_controller()
    }

    fn host_controller(&self) -> &Arc<HostController> {
        self.0.host_controller()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        self.0.client_actor_index()
    }

    fn public_key(&self) -> &DecodingKey {
        self.0.public_key()
    }

    fn private_key(&self) -> &EncodingKey {
        self.0.private_key()
    }

    fn sendgrid_controller(&self) -> Option<&SendGridController> {
        self.0.sendgrid_controller()
    }
}

impl<T: NodeDelegate + ?Sized> NodeDelegate for Arc<T> {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        (**self).gather_metrics()
    }

    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController {
        (**self).database_instance_context_controller()
    }

    fn host_controller(&self) -> &Arc<HostController> {
        (**self).host_controller()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        (**self).client_actor_index()
    }

    fn public_key(&self) -> &DecodingKey {
        (**self).public_key()
    }

    fn private_key(&self) -> &EncodingKey {
        (**self).private_key()
    }

    fn sendgrid_controller(&self) -> Option<&SendGridController> {
        (**self).sendgrid_controller()
    }
}

#[async_trait]
impl<T: ModuleHostContextLoader + ?Sized> ModuleHostContextLoader for ArcEnv<T> {
    async fn load_module_host_context(&self, db: Database, instance_id: u64) -> anyhow::Result<ModuleHostContext> {
        self.0.load_module_host_context(db, instance_id).await
    }
}

#[async_trait]
impl<T: ModuleHostContextLoader + ?Sized> ModuleHostContextLoader for Arc<T> {
    async fn load_module_host_context(&self, db: Database, instance_id: u64) -> anyhow::Result<ModuleHostContext> {
        (**self).load_module_host_context(db, instance_id).await
    }
}

pub fn log_and_500(e: impl std::fmt::Display) -> StatusCode {
    log::error!("internal error: {e:#}");
    StatusCode::INTERNAL_SERVER_ERROR
}
