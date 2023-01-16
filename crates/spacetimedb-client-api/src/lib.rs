use async_trait::async_trait;
use spacetimedb::address::Address;
use spacetimedb::client::client_connection_index;
use spacetimedb::hash::Hash;
use spacetimedb::protobuf::control_db::{Database, DatabaseInstance, HostType};
use spacetimedb::protobuf::control_worker_api::ScheduleState;
use spacetimedb::protobuf::worker_db::DatabaseInstanceState;
use tokio::net::{TcpListener, ToSocketAddrs};
mod auth;
mod routes;
use std::future;
use std::sync::Arc;

use routes::router;

pub async fn start(db: Arc<dyn ApiCtx>, addr: impl ToSocketAddrs) -> ! {
    start_customized(db, addr, |_| {}).await
}

pub async fn start_control(
    db: Arc<impl ControllerCtx + 'static>,
    addr: impl ToSocketAddrs,
    customize: impl FnOnce(&mut gotham::router::builder::RouterBuilder<'_, (), ()>),
) -> ! {
    _start(router(db.clone(), Some(db), customize), addr).await
}

pub async fn start_customized(
    db: Arc<dyn ApiCtx>,
    addr: impl ToSocketAddrs,
    customize: impl FnOnce(&mut gotham::router::builder::RouterBuilder<'_, (), ()>),
) -> ! {
    _start(router(db, None, customize), addr).await
}

pub async fn _start(route: gotham::router::Router, addr: impl ToSocketAddrs) -> ! {
    client_connection_index::ClientActorIndex::start_liveliness_check();

    let tcp = TcpListener::bind(addr).await.unwrap();

    log::debug!("Starting client API listening on {}", tcp.local_addr().unwrap());
    gotham::bind_server(tcp, route, |s| future::ready(Ok(s))).await
}

pub trait ControllerCtx: Controller + ApiCtx {}
impl<T: Controller + ApiCtx> ControllerCtx for T {}

#[async_trait]
pub trait Controller: Send + Sync {
    async fn insert_database(
        &self,
        address: &Address,
        identity: &Hash,
        program_bytes_address: &Hash,
        host_type: HostType,
        num_replicas: u32,
        force: bool,
        trace_log: bool,
    ) -> Result<(), anyhow::Error>;

    async fn update_database(
        &self,
        address: &Address,
        program_bytes_address: &Hash,
        num_replicas: u32,
    ) -> Result<(), anyhow::Error>;

    async fn delete_database(&self, address: &Address) -> Result<(), anyhow::Error>;
}

pub trait ApiCtx: DatabaseDb {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily>;
}

#[async_trait]
pub trait DatabaseDb: Send + Sync {
    async fn set_node_id(&self, node_id: u64) -> Result<(), anyhow::Error>;

    async fn get_node_id(&self) -> Result<Option<u64>, anyhow::Error>;

    async fn upsert_database_instance_state(&self, state: DatabaseInstanceState) -> Result<(), anyhow::Error>;

    async fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> Result<Option<DatabaseInstanceState>, anyhow::Error>;

    async fn init_with_schedule_state(&self, schedule_state: ScheduleState);

    async fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>>;

    async fn get_database_by_address(&self, address: &Address) -> anyhow::Result<Option<Database>>;

    async fn _get_databases(&self) -> anyhow::Result<Vec<Database>>;

    async fn insert_database(&self, database: Database) -> anyhow::Result<u64>;

    async fn delete_database(&self, database_id: u64) -> anyhow::Result<Option<u64>>;

    async fn _get_database_instance_by_id(&self, id: u64) -> anyhow::Result<Option<DatabaseInstance>>;

    async fn get_database_instances(&self) -> anyhow::Result<Vec<DatabaseInstance>>;

    async fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance>;

    async fn insert_database_instance(&self, database_instance: DatabaseInstance) -> anyhow::Result<u64>;

    async fn delete_database_instance(&self, database_instance_id: u64) -> anyhow::Result<()>;
}

#[macro_export]
macro_rules! delegate_controller {
    (for $t:ty, $self:ident to $target:expr) => {
        #[async_trait::async_trait]
        impl $crate::Controller for $t {
            async fn insert_database(
                &$self,
                address: &spacetimedb::address::Address,
                identity: &spacetimedb::hash::Hash,
                program_bytes_address: &spacetimedb::hash::Hash,
                host_type: spacetimedb::protobuf::control_db::HostType,
                num_replicas: u32,
                force: bool,
                trace_log: bool,
            ) -> Result<(), anyhow::Error> {
                $target.insert_database(
                    address,
                    identity,
                    program_bytes_address,
                    host_type,
                    num_replicas,
                    force,
                    trace_log,
                )
                .await
            }

            async fn update_database(
                &$self,
                address: &spacetimedb::address::Address,
                program_bytes_address: &spacetimedb::hash::Hash,
                num_replicas: u32,
            ) -> Result<(), anyhow::Error> {
                $target.update_database(
                    address,
                    program_bytes_address,
                    num_replicas,
                )
                .await
            }

            async fn delete_database(&$self, address: &spacetimedb::address::Address) -> Result<(), anyhow::Error> {
                $target.delete_database(address).await
            }
        }
    };
}

#[macro_export]
macro_rules! delegate_databasedb {
    (for $t:ty, $self:ident to $target:expr$(, |$x:ident| $map:expr)?) => {
        #[async_trait::async_trait]
        impl $crate::DatabaseDb for $t {
            async fn set_node_id(&$self, node_id: u64) -> Result<(), anyhow::Error> {
                let x = $target.set_node_id(node_id);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn get_node_id(&$self) -> Result<Option<u64>, anyhow::Error> {
                let x = $target.get_node_id();
                $(let x = match x { $x => $map };)?
                x
            }

            async fn upsert_database_instance_state(
                &$self,
                state: spacetimedb::protobuf::worker_db::DatabaseInstanceState,
            ) -> Result<(), anyhow::Error> {
                let x = $target.upsert_database_instance_state(state);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn get_database_instance_state(
                &$self,
                database_instance_id: u64,
            ) -> Result<Option<spacetimedb::protobuf::worker_db::DatabaseInstanceState>, anyhow::Error> {
                let x = $target.get_database_instance_state(database_instance_id);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn init_with_schedule_state(
                &$self,
                schedule_state: spacetimedb::protobuf::control_worker_api::ScheduleState,
            ) {
                let x = $target.init_with_schedule_state(schedule_state);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn get_database_by_id(
                &$self,
                id: u64,
            ) -> anyhow::Result<Option<spacetimedb::protobuf::control_db::Database>> {
                let x = $target.get_database_by_id(id);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn get_database_by_address(
                &$self,
                address: &spacetimedb::address::Address,
            ) -> anyhow::Result<Option<spacetimedb::protobuf::control_db::Database>> {
                let x = $target.get_database_by_address(address);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn _get_databases(&$self) -> anyhow::Result<Vec<spacetimedb::protobuf::control_db::Database>> {
                let x = $target._get_databases();
                $(let x = match x { $x => $map };)?
                x
            }

            async fn insert_database(
                &$self,
                database: spacetimedb::protobuf::control_db::Database,
            ) -> anyhow::Result<u64> {
                let x = $target.insert_database(database);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn delete_database(&$self, database_id: u64) -> anyhow::Result<Option<u64>> {
                let x = $target.delete_database(database_id);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn _get_database_instance_by_id(
                &$self,
                id: u64,
            ) -> anyhow::Result<Option<spacetimedb::protobuf::control_db::DatabaseInstance>> {
                let x = $target._get_database_instance_by_id(id);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn get_database_instances(
                &$self,
            ) -> anyhow::Result<Vec<spacetimedb::protobuf::control_db::DatabaseInstance>> {
                let x = $target.get_database_instances();
                $(let x = match x { $x => $map };)?
                x
            }

            async fn get_leader_database_instance_by_database(
                &$self,
                database_id: u64,
            ) -> Option<spacetimedb::protobuf::control_db::DatabaseInstance> {
                let x = $target.get_leader_database_instance_by_database(database_id);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn insert_database_instance(
                &$self,
                database_instance: spacetimedb::protobuf::control_db::DatabaseInstance,
            ) -> anyhow::Result<u64> {
                let x = $target.insert_database_instance(database_instance);
                $(let x = match x { $x => $map };)?
                x
            }

            async fn delete_database_instance(&$self, database_instance_id: u64) -> anyhow::Result<()> {
                let x = $target.delete_database_instance(database_instance_id);
                $(let x = match x { $x => $map };)?
                x
            }
        }
    };
}

delegate_databasedb!(for spacetimedb::control_db::ControlDb, self to self, |x| x.await);
