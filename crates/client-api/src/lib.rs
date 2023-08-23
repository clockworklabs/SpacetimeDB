use async_trait::async_trait;
use axum::extract::FromRef;
use http::StatusCode;
use spacetimedb::address::Address;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::control_db::ControlDb;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::hash::Hash;
use spacetimedb::host::UpdateDatabaseResult;
use spacetimedb::host::{EnergyQuanta, HostController};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, DatabaseInstance, HostType, Node};
use spacetimedb::messages::worker_db::DatabaseInstanceState;
use spacetimedb::module_host_context::ModuleHostContext;
use spacetimedb::object_db::ObjectDb;
use spacetimedb::sendgrid_controller::SendGridController;
use spacetimedb_lib::name::DomainName;
mod auth;
pub mod routes;
pub mod util;
use std::sync::Arc;

#[async_trait]
pub trait WorkerCtx: ControlNodeDelegate + ControlStateDelegate + Send + Sync {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily>;
    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController;
    async fn load_module_host_context(&self, db: Database, instance_id: u64) -> anyhow::Result<ModuleHostContext>;
    fn host_controller(&self) -> &Arc<HostController>;
    fn client_actor_index(&self) -> &ClientActorIndex;
}

#[async_trait]
pub trait ControlStateDelegate: Send + Sync {
    async fn get_node_id(&self) -> Result<Option<u64>, anyhow::Error>;

    async fn get_node_by_id(&self, node_id: u64) -> spacetimedb::control_db::Result<Option<Node>>;

    async fn get_nodes(&self) -> spacetimedb::control_db::Result<Vec<Node>>;

    async fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> Result<Option<DatabaseInstanceState>, anyhow::Error>;

    async fn get_database_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<Database>>;

    async fn get_database_by_address(&self, address: &Address) -> spacetimedb::control_db::Result<Option<Database>>;

    async fn get_databases(&self) -> spacetimedb::control_db::Result<Vec<Database>>;

    async fn get_database_instance_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<DatabaseInstance>>;

    async fn get_database_instances(&self) -> spacetimedb::control_db::Result<Vec<DatabaseInstance>>;

    async fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance>;
}

#[async_trait]
pub trait ControlCtx: ControlNodeDelegate + Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn insert_database(
        &self,
        address: &Address,
        identity: &Identity,
        program_bytes_address: &Hash,
        host_type: HostType,
        num_replicas: u32,
        force: bool,
    ) -> Result<(), anyhow::Error>;

    async fn update_database(
        &self,
        address: &Address,
        program_bytes_address: &Hash,
        num_replicas: u32,
    ) -> Result<Option<UpdateDatabaseResult>, anyhow::Error>;

    async fn delete_database(&self, address: &Address) -> Result<(), anyhow::Error>;

    fn object_db(&self) -> &ObjectDb;
    fn control_db(&self) -> &ControlDb;
    fn sendgrid_controller(&self) -> Option<&SendGridController>;
}

#[async_trait]
pub trait ControlNodeDelegate: Send + Sync {
    async fn spacetime_dns(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>>;

    async fn alloc_spacetime_identity(&self) -> spacetimedb::control_db::Result<Identity>;

    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> spacetimedb::control_db::Result<()>;

    fn public_key(&self) -> &DecodingKey;
    fn private_key(&self) -> &EncodingKey;
}

pub struct ArcEnv<T: ?Sized>(pub Arc<T>);
impl<T: ?Sized> Clone for ArcEnv<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
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
    async fn spacetime_dns(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>> {
        self.0.spacetime_dns(domain).await
    }

    async fn alloc_spacetime_identity(&self) -> spacetimedb::control_db::Result<Identity> {
        self.0.alloc_spacetime_identity().await
    }

    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> spacetimedb::control_db::Result<()> {
        self.0.withdraw_energy(identity, amount).await
    }

    fn public_key(&self) -> &DecodingKey {
        self.0.public_key()
    }
    fn private_key(&self) -> &EncodingKey {
        self.0.private_key()
    }
}

#[async_trait]
impl<T: ControlNodeDelegate + ?Sized> ControlNodeDelegate for Arc<T> {
    async fn spacetime_dns(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>> {
        (**self).spacetime_dns(domain).await
    }

    async fn alloc_spacetime_identity(&self) -> spacetimedb::control_db::Result<Identity> {
        (**self).alloc_spacetime_identity().await
    }

    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> spacetimedb::control_db::Result<()> {
        (**self).withdraw_energy(identity, amount).await
    }

    fn public_key(&self) -> &DecodingKey {
        (**self).public_key()
    }
    fn private_key(&self) -> &EncodingKey {
        (**self).private_key()
    }
}

pub fn log_and_500(e: impl std::fmt::Display) -> StatusCode {
    log::error!("internal error: {e:#}");
    StatusCode::INTERNAL_SERVER_ERROR
}
