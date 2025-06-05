use std::num::NonZeroU8;
use std::sync::Arc;

use async_trait::async_trait;
use axum::response::ErrorResponse;
use http::StatusCode;

use spacetimedb::client::ClientActorIndex;
use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
use spacetimedb::host::{HostController, ModuleHost, NoSuchModule, UpdateDatabaseResult};
use spacetimedb::identity::{AuthCtx, Identity};
use spacetimedb::messages::control_db::{Database, HostType, Node, Replica};
use spacetimedb::sql;
use spacetimedb_client_api_messages::http::{SqlStmtResult, SqlStmtStats};
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, SetDomainsResult, Tld};
use spacetimedb_lib::{ProductTypeElement, ProductValue};
use spacetimedb_paths::server::ModuleLogsDir;
use tokio::sync::watch;

pub mod auth;
pub mod routes;
pub mod util;

/// Defines the state / environment of a SpacetimeDB node from the PoV of the
/// client API.
///
/// Types returned here should be considered internal state and **never** be
/// surfaced to the API.
#[async_trait]
pub trait NodeDelegate: Send + Sync {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily>;
    fn client_actor_index(&self) -> &ClientActorIndex;

    type JwtAuthProviderT: auth::JwtAuthProvider;
    fn jwt_auth_provider(&self) -> &Self::JwtAuthProviderT;
    /// Return the leader [`Host`] of `database_id`.
    ///
    /// Returns `None` if the current leader is not hosted by this node.
    /// The [`Host`] is spawned implicitly if not already running.
    async fn leader(&self, database_id: u64) -> anyhow::Result<Option<Host>>;
    fn module_logs_dir(&self, replica_id: u64) -> ModuleLogsDir;
}

/// Client view of a running module.
pub struct Host {
    pub replica_id: u64,
    host_controller: HostController,
}

impl Host {
    pub fn new(replica_id: u64, host_controller: HostController) -> Self {
        Self {
            replica_id,
            host_controller,
        }
    }

    pub async fn module(&self) -> Result<ModuleHost, NoSuchModule> {
        self.host_controller.get_module_host(self.replica_id).await
    }

    pub async fn module_watcher(&self) -> Result<watch::Receiver<ModuleHost>, NoSuchModule> {
        self.host_controller.watch_module_host(self.replica_id).await
    }

    pub async fn exec_sql(
        &self,
        auth: AuthCtx,
        database: Database,
        body: String,
    ) -> axum::response::Result<Vec<SqlStmtResult<ProductValue>>> {
        let module_host = self
            .module()
            .await
            .map_err(|_| (StatusCode::NOT_FOUND, "module not found".to_string()))?;

        let json = self
            .host_controller
            .using_database(
                database,
                self.replica_id,
                move |db| -> axum::response::Result<_, (StatusCode, String)> {
                    tracing::info!(sql = body);

                    // We need a header for query results
                    let mut header = vec![];

                    let sql_start = std::time::Instant::now();
                    let sql_span =
                        tracing::trace_span!("execute_sql", total_duration = tracing::field::Empty,).entered();

                    let result = sql::execute::run(
                        // Returns an empty result set for mutations
                        db,
                        &body,
                        auth,
                        Some(&module_host.info().subscriptions),
                        &mut header,
                    )
                    .map_err(|e| {
                        log::warn!("{}", e);
                        if let Some(auth_err) = e.get_auth_error() {
                            (StatusCode::UNAUTHORIZED, auth_err.to_string())
                        } else {
                            (StatusCode::BAD_REQUEST, e.to_string())
                        }
                    })?;

                    let total_duration = sql_start.elapsed();
                    sql_span.record("total_duration", tracing::field::debug(total_duration));

                    // Turn the header into a `ProductType`
                    let schema = header
                        .into_iter()
                        .map(|(col_name, col_type)| ProductTypeElement::new(col_type, Some(col_name)))
                        .collect();

                    Ok(vec![SqlStmtResult {
                        schema,
                        rows: result.rows,
                        total_duration_micros: total_duration.as_micros() as u64,
                        stats: SqlStmtStats::from_metrics(&result.metrics),
                    }])
                },
            )
            .await
            .map_err(log_and_500)??;

        Ok(json)
    }

    pub async fn update(
        &self,
        database: Database,
        host_type: HostType,
        program_bytes: Box<[u8]>,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        self.host_controller
            .update_module_host(database, host_type, self.replica_id, program_bytes)
            .await
    }
}

/// Parameters for publishing a database.
///
/// See [`ControlStateDelegate::publish_database`].
pub struct DatabaseDef {
    /// The [`Identity`] the database shall have.
    pub database_identity: Identity,
    /// The compiled program of the database module.
    pub program_bytes: Vec<u8>,
    /// The desired number of replicas the database shall have.
    ///
    /// If `None`, the edition default is used.
    pub num_replicas: Option<NonZeroU8>,
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
    fn lookup_identity(&self, domain: &str) -> anyhow::Result<Option<Identity>>;
    fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>>;
}

/// Write operations on the SpacetimeDB control plane.
#[async_trait]
pub trait ControlStateWriteAccess: Send + Sync {
    /// Publish a database acc. to [`DatabaseDef`].
    ///
    /// If the database with the given identity was successfully published before,
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

    async fn delete_database(&self, caller_identity: &Identity, database_identity: &Identity) -> anyhow::Result<()>;

    // Energy
    async fn add_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()>;
    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()>;

    // DNS
    async fn register_tld(&self, identity: &Identity, tld: Tld) -> anyhow::Result<RegisterTldResult>;
    async fn create_dns_record(
        &self,
        owner_identity: &Identity,
        domain: &DomainName,
        database_identity: &Identity,
    ) -> anyhow::Result<InsertDomainResult>;

    /// Replace all dns records pointing to `database_identity` with `domain_names`.
    ///
    /// All existing names in the database and in `domain_names` must be
    /// owned by `owner_identity` (i.e. their TLD must belong to `owner_identity`).
    ///
    /// The `owner_identity` is typically also the owner of the database.
    ///
    /// Note that passing an empty slice is legal, and will just remove any
    /// existing dns records.
    async fn replace_dns_records(
        &self,
        database_identity: &Identity,
        owner_identity: &Identity,
        domain_names: &[DomainName],
    ) -> anyhow::Result<SetDomainsResult>;
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
    fn get_database_by_identity(&self, identity: &Identity) -> anyhow::Result<Option<Database>> {
        (**self).get_database_by_identity(identity)
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

    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
        (**self).get_energy_balance(identity)
    }

    // DNS
    fn lookup_identity(&self, domain: &str) -> anyhow::Result<Option<Identity>> {
        (**self).lookup_identity(domain)
    }

    fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>> {
        (**self).reverse_lookup(database_identity)
    }

    fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica> {
        (**self).get_leader_replica_by_database(database_id)
    }
}

#[async_trait]
impl<T: ControlStateWriteAccess + ?Sized> ControlStateWriteAccess for Arc<T> {
    async fn publish_database(
        &self,
        identity: &Identity,
        spec: DatabaseDef,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        (**self).publish_database(identity, spec).await
    }

    async fn delete_database(&self, caller_identity: &Identity, database_identity: &Identity) -> anyhow::Result<()> {
        (**self).delete_database(caller_identity, database_identity).await
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
        database_identity: &Identity,
    ) -> anyhow::Result<InsertDomainResult> {
        (**self).create_dns_record(identity, domain, database_identity).await
    }

    async fn replace_dns_records(
        &self,
        database_identity: &Identity,
        owner_identity: &Identity,
        domain_names: &[DomainName],
    ) -> anyhow::Result<SetDomainsResult> {
        (**self)
            .replace_dns_records(database_identity, owner_identity, domain_names)
            .await
    }
}

#[async_trait]
impl<T: NodeDelegate + ?Sized> NodeDelegate for Arc<T> {
    type JwtAuthProviderT = T::JwtAuthProviderT;
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        (**self).gather_metrics()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        (**self).client_actor_index()
    }

    fn jwt_auth_provider(&self) -> &Self::JwtAuthProviderT {
        (**self).jwt_auth_provider()
    }

    async fn leader(&self, database_id: u64) -> anyhow::Result<Option<Host>> {
        (**self).leader(database_id).await
    }

    fn module_logs_dir(&self, replica_id: u64) -> ModuleLogsDir {
        (**self).module_logs_dir(replica_id)
    }
}

pub fn log_and_500(e: impl std::fmt::Display) -> ErrorResponse {
    log::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into()
}
