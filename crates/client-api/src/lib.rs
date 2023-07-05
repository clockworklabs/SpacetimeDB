use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::FromRef;
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
use spacetimedb_lib::name::{DomainName, RegisterTldResult, Tld};
use spacetimedb_lib::recovery::RecoveryCode;

pub mod auth;
pub mod routes;
pub mod util;

// TODO(kim): Changes in the cloud architecture made the distinction between
// worker and control basically irrelevant. We could probably collapse the
// traits below into `NodeDelegate` (access to internals) and `ControlDelegate`
// (control plane access).

#[async_trait]
pub trait WorkerCtx: WorkerNodeDelegate + ControlNodeDelegate + ControlStateDelegate + Send + Sync {}
#[async_trait]
pub trait ControlCtx: ControlNodeDelegate + ControlStateDelegate + Send + Sync {}

impl<T: WorkerCtx + Send + Sync> ControlCtx for T {}

#[async_trait]
pub trait WorkerNodeDelegate: Send + Sync {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily>;
    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController;
    fn host_controller(&self) -> &Arc<HostController>;
    fn client_actor_index(&self) -> &ClientActorIndex;

    async fn load_module_host_context(&self, db: Database, instance_id: u64) -> anyhow::Result<ModuleHostContext>;
}

#[async_trait]
pub trait ControlNodeDelegate: Send + Sync {
    fn public_key(&self) -> &DecodingKey;
    fn private_key(&self) -> &EncodingKey;
    fn sendgrid_controller(&self) -> Option<&SendGridController>;
}

pub struct DatabaseDef {
    pub address: Address,
    pub program_bytes: Vec<u8>,
    pub num_replicas: u32,
    pub trace_log: bool,
}

#[async_trait]
pub trait ControlStateDelegate: Send + Sync {
    // Nodes
    async fn get_node_id(&self) -> Option<u64>;
    async fn get_node_by_id(&self, node_id: u64) -> spacetimedb::control_db::Result<Option<Node>>;
    async fn get_nodes(&self) -> spacetimedb::control_db::Result<Vec<Node>>;

    // Databases
    async fn get_database_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<Database>>;
    async fn get_database_by_address(&self, address: &Address) -> spacetimedb::control_db::Result<Option<Database>>;
    async fn get_databases(&self) -> spacetimedb::control_db::Result<Vec<Database>>;

    async fn create_address(&self) -> spacetimedb::control_db::Result<Address>;
    async fn publish_database(&self, identity: &Identity, spec: DatabaseDef) -> spacetimedb::control_db::Result<()>;
    async fn delete_database(&self, identity: &Identity, address: &Address) -> spacetimedb::control_db::Result<()>;

    // Database instances
    async fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> spacetimedb::control_db::Result<Option<DatabaseInstanceState>>;
    async fn get_database_instance_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<DatabaseInstance>>;
    async fn get_database_instances(&self) -> spacetimedb::control_db::Result<Vec<DatabaseInstance>>;
    async fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance>;

    // Identities
    async fn get_identities_for_email(&self, email: &str) -> spacetimedb::control_db::Result<Vec<IdentityEmail>>;
    async fn get_recovery_codes(&self, email: &str) -> spacetimedb::control_db::Result<Vec<RecoveryCode>>;

    async fn create_identity(&self) -> spacetimedb::control_db::Result<Identity>;
    async fn add_email(&self, identity: &Identity, email: &str) -> spacetimedb::control_db::Result<()>;
    async fn insert_recovery_code(
        &self,
        identity: &Identity,
        email: &str,
        code: RecoveryCode,
    ) -> spacetimedb::control_db::Result<()>;

    // Energy
    async fn get_energy_balance(&self, identity: &Identity) -> spacetimedb::control_db::Result<Option<EnergyBalance>>;
    async fn add_energy(&self, identity: &Identity, quanta: u64) -> spacetimedb::control_db::Result<()>;

    // DNS
    async fn lookup_address(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>>;
    async fn reverse_lookup(&self, address: &Address) -> spacetimedb::control_db::Result<Vec<DomainName>>;

    async fn register_tld(&self, identity: &Identity, tld: Tld) -> spacetimedb::control_db::Result<RegisterTldResult>;
    async fn create_dns_record(
        &self,
        identity: &Identity,
        domain: &DomainName,
        address: &Address,
    ) -> spacetimedb::control_db::Result<()>;
}

pub struct ArcEnv<T: ?Sized>(pub Arc<T>);
impl<T: ?Sized> Clone for ArcEnv<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[async_trait::async_trait]
impl<T: ControlStateDelegate + ?Sized> ControlStateDelegate for ArcEnv<T> {
    // Nodes
    async fn get_node_id(&self) -> Option<u64> {
        self.0.get_node_id().await
    }
    async fn get_node_by_id(&self, node_id: u64) -> spacetimedb::control_db::Result<Option<Node>> {
        self.0.get_node_by_id(node_id).await
    }
    async fn get_nodes(&self) -> spacetimedb::control_db::Result<Vec<Node>> {
        self.0.get_nodes().await
    }

    // Databases
    async fn get_database_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<Database>> {
        self.0.get_database_by_id(id).await
    }
    async fn get_database_by_address(&self, address: &Address) -> spacetimedb::control_db::Result<Option<Database>> {
        self.0.get_database_by_address(address).await
    }
    async fn get_databases(&self) -> spacetimedb::control_db::Result<Vec<Database>> {
        self.0.get_databases().await
    }

    async fn create_address(&self) -> spacetimedb::control_db::Result<Address> {
        self.0.create_address().await
    }

    async fn publish_database(&self, identity: &Identity, spec: DatabaseDef) -> spacetimedb::control_db::Result<()> {
        self.0.publish_database(identity, spec).await
    }
    async fn delete_database(&self, identity: &Identity, address: &Address) -> spacetimedb::control_db::Result<()> {
        self.0.delete_database(identity, address).await
    }

    // Database instances
    async fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> spacetimedb::control_db::Result<Option<DatabaseInstanceState>> {
        self.0.get_database_instance_state(database_instance_id).await
    }
    async fn get_database_instance_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<DatabaseInstance>> {
        self.0.get_database_instance_by_id(id).await
    }
    async fn get_database_instances(&self) -> spacetimedb::control_db::Result<Vec<DatabaseInstance>> {
        self.0.get_database_instances().await
    }
    async fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.get_leader_database_instance_by_database(database_id).await
    }

    // Identities
    async fn get_identities_for_email(&self, email: &str) -> spacetimedb::control_db::Result<Vec<IdentityEmail>> {
        self.0.get_identities_for_email(email).await
    }
    async fn get_recovery_codes(&self, email: &str) -> spacetimedb::control_db::Result<Vec<RecoveryCode>> {
        self.0.get_recovery_codes(email).await
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

    // Energy
    async fn get_energy_balance(&self, identity: &Identity) -> spacetimedb::control_db::Result<Option<EnergyBalance>> {
        self.0.get_energy_balance(identity).await
    }
    async fn add_energy(&self, identity: &Identity, quanta: u64) -> spacetimedb::control_db::Result<()> {
        self.0.add_energy(identity, quanta).await
    }

    // DNS
    async fn lookup_address(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>> {
        self.0.lookup_address(domain).await
    }
    async fn reverse_lookup(&self, address: &Address) -> spacetimedb::control_db::Result<Vec<DomainName>> {
        self.0.reverse_lookup(address).await
    }

    async fn register_tld(&self, identity: &Identity, tld: Tld) -> spacetimedb::control_db::Result<RegisterTldResult> {
        self.0.register_tld(identity, tld).await
    }
    async fn create_dns_record(
        &self,
        identity: &Identity,
        domain: &DomainName,
        address: &Address,
    ) -> spacetimedb::control_db::Result<()> {
        self.0.create_dns_record(identity, domain, address).await
    }
}

impl<T: ControlCtx + 'static> FromRef<ArcEnv<T>> for Arc<dyn ControlCtx> {
    fn from_ref(env: &ArcEnv<T>) -> Self {
        env.0.clone()
    }
}

impl<T: WorkerCtx + 'static> FromRef<ArcEnv<T>> for Arc<dyn WorkerCtx> {
    fn from_ref(env: &ArcEnv<T>) -> Self {
        env.0.clone()
    }
}

#[async_trait]
impl<T: ControlNodeDelegate + ?Sized> ControlNodeDelegate for ArcEnv<T> {
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

#[async_trait]
impl<T: ControlNodeDelegate + ?Sized> ControlNodeDelegate for Arc<T> {
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

pub fn log_and_500(e: impl std::fmt::Display) -> StatusCode {
    log::error!("internal error: {e:#}");
    StatusCode::INTERNAL_SERVER_ERROR
}
