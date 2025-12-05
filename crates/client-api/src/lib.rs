use std::fmt;
use std::future::Future;
use std::num::NonZeroU8;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use axum::response::ErrorResponse;
use bytes::Bytes;
use http::StatusCode;

use spacetimedb::client::ClientActorIndex;
use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
use spacetimedb::host::{HostController, MigratePlanResult, ModuleHost, NoSuchModule, UpdateDatabaseResult};
use spacetimedb::identity::{AuthCtx, Identity};
use spacetimedb::messages::control_db::{Database, HostType, Node, Replica};
use spacetimedb::sql;
use spacetimedb_client_api_messages::http::{SqlStmtResult, SqlStmtStats};
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, SetDomainsResult, Tld};
use spacetimedb_lib::{ProductTypeElement, ProductValue};
use spacetimedb_paths::server::ModuleLogsDir;
use spacetimedb_schema::auto_migrate::{MigrationPolicy, PrettyPrintStyle};
use thiserror::Error;
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
        confirmed_read: bool,
        body: String,
    ) -> axum::response::Result<Vec<SqlStmtResult<ProductValue>>> {
        let module_host = self
            .module()
            .await
            .map_err(|_| (StatusCode::NOT_FOUND, "module not found".to_string()))?;

        let (tx_offset, durable_offset, json) = self
            .host_controller
            .using_database(database, self.replica_id, move |db| async move {
                tracing::info!(sql = body);
                let mut header = vec![];
                let sql_start = std::time::Instant::now();
                let sql_span = tracing::trace_span!("execute_sql", total_duration = tracing::field::Empty,);
                let _guard = sql_span.enter();

                let result = sql::execute::run(
                    &db,
                    &body,
                    auth,
                    Some(&module_host.info.subscriptions),
                    Some(&module_host),
                    &mut header,
                )
                .await
                .map_err(|e| {
                    log::warn!("{e}");
                    if let Some(auth_err) = e.get_auth_error() {
                        (StatusCode::UNAUTHORIZED, auth_err.to_string())
                    } else {
                        (StatusCode::BAD_REQUEST, e.to_string())
                    }
                })?;

                let total_duration = sql_start.elapsed();
                drop(_guard);
                sql_span.record("total_duration", tracing::field::debug(total_duration));

                let schema = header
                    .into_iter()
                    .map(|(col_name, col_type)| ProductTypeElement::new(col_type, Some(col_name)))
                    .collect();

                Ok::<_, (StatusCode, String)>((
                    result.tx_offset,
                    db.durable_tx_offset(),
                    vec![SqlStmtResult {
                        schema,
                        rows: result.rows,
                        total_duration_micros: total_duration.as_micros() as u64,
                        stats: SqlStmtStats::from_metrics(&result.metrics),
                    }],
                ))
            })
            .await
            .map_err(log_and_500)??;

        if confirmed_read && let Some(mut durable_offset) = durable_offset {
            let tx_offset = tx_offset.await.map_err(|_| log_and_500("transaction aborted"))?;
            durable_offset.wait_for(tx_offset).await.map_err(log_and_500)?;
        }

        Ok(json)
    }

    pub async fn update(
        &self,
        database: Database,
        host_type: HostType,
        program_bytes: Box<[u8]>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        self.host_controller
            .update_module_host(database, host_type, self.replica_id, program_bytes, policy)
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
    pub program_bytes: Bytes,
    /// The desired number of replicas the database shall have.
    ///
    /// If `None`, the edition default is used.
    pub num_replicas: Option<NonZeroU8>,
    /// The host type of the supplied program.
    pub host_type: HostType,
    pub parent: Option<Identity>,
}

/// Parameters for resetting a database via [`ControlStateDelegate::reset_database`].
pub struct DatabaseResetDef {
    pub database_identity: Identity,
    pub program_bytes: Option<Bytes>,
    pub num_replicas: Option<NonZeroU8>,
    pub host_type: Option<HostType>,
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
#[async_trait]
pub trait ControlStateReadAccess {
    // Nodes
    async fn get_node_id(&self) -> Option<u64>;
    async fn get_node_by_id(&self, node_id: u64) -> anyhow::Result<Option<Node>>;
    async fn get_nodes(&self) -> anyhow::Result<Vec<Node>>;

    // Databases
    async fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>>;
    async fn get_database_by_identity(&self, database_identity: &Identity) -> anyhow::Result<Option<Database>>;
    async fn get_databases(&self) -> anyhow::Result<Vec<Database>>;

    // Replicas
    async fn get_replica_by_id(&self, id: u64) -> anyhow::Result<Option<Replica>>;
    async fn get_replicas(&self) -> anyhow::Result<Vec<Replica>>;
    async fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica>;

    // Energy
    async fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>>;

    // DNS
    async fn lookup_identity(&self, domain: &str) -> anyhow::Result<Option<Identity>>;
    async fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>>;
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
        policy: MigrationPolicy,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>>;

    async fn migrate_plan(&self, spec: DatabaseDef, style: PrettyPrintStyle) -> anyhow::Result<MigratePlanResult>;

    async fn delete_database(&self, caller_identity: &Identity, database_identity: &Identity) -> anyhow::Result<()>;

    /// Remove all data from a database, and reset it according to the
    /// given [DatabaseResetDef].
    async fn reset_database(&self, caller_identity: &Identity, spec: DatabaseResetDef) -> anyhow::Result<()>;

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

#[async_trait]
impl<T: ControlStateReadAccess + Send + Sync + Sync + ?Sized> ControlStateReadAccess for Arc<T> {
    // Nodes
    async fn get_node_id(&self) -> Option<u64> {
        (**self).get_node_id().await
    }
    async fn get_node_by_id(&self, node_id: u64) -> anyhow::Result<Option<Node>> {
        (**self).get_node_by_id(node_id).await
    }
    async fn get_nodes(&self) -> anyhow::Result<Vec<Node>> {
        (**self).get_nodes().await
    }

    // Databases
    async fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>> {
        (**self).get_database_by_id(id).await
    }
    async fn get_database_by_identity(&self, identity: &Identity) -> anyhow::Result<Option<Database>> {
        (**self).get_database_by_identity(identity).await
    }
    async fn get_databases(&self) -> anyhow::Result<Vec<Database>> {
        (**self).get_databases().await
    }

    // Replicas
    async fn get_replica_by_id(&self, id: u64) -> anyhow::Result<Option<Replica>> {
        (**self).get_replica_by_id(id).await
    }
    async fn get_replicas(&self) -> anyhow::Result<Vec<Replica>> {
        (**self).get_replicas().await
    }

    // Energy
    async fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
        (**self).get_energy_balance(identity).await
    }

    // DNS
    async fn lookup_identity(&self, domain: &str) -> anyhow::Result<Option<Identity>> {
        (**self).lookup_identity(domain).await
    }

    async fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>> {
        (**self).reverse_lookup(database_identity).await
    }

    async fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica> {
        (**self).get_leader_replica_by_database(database_id).await
    }
}

#[async_trait]
impl<T: ControlStateWriteAccess + ?Sized> ControlStateWriteAccess for Arc<T> {
    async fn publish_database(
        &self,
        identity: &Identity,
        spec: DatabaseDef,
        policy: MigrationPolicy,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        (**self).publish_database(identity, spec, policy).await
    }

    async fn migrate_plan(&self, spec: DatabaseDef, style: PrettyPrintStyle) -> anyhow::Result<MigratePlanResult> {
        (**self).migrate_plan(spec, style).await
    }

    async fn delete_database(&self, caller_identity: &Identity, database_identity: &Identity) -> anyhow::Result<()> {
        (**self).delete_database(caller_identity, database_identity).await
    }

    async fn reset_database(&self, caller_identity: &Identity, spec: DatabaseResetDef) -> anyhow::Result<()> {
        (**self).reset_database(caller_identity, spec).await
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

/// Result of an authorization check performed by an implementation of the
/// [Authorization] trait.
///
/// [Unauthorized::Unauthorized] means that the subject was denied the
/// permission to perform the requested action.
///
/// [Unauthorized::InternalError] indicates an error to perform the check in
/// the first place. It may succeed when retried.
///
/// The [axum::response::IntoResponse] impl maps the variants to HTTP responses
/// as follows:
///
/// * [Unauthorized::InternalError] is mapped to a 503 Internal Server Error
///   response with the inner error sent as a string in the response body.
///
/// * [Unauthorized::Unauthorized] is mapped to a 403 Forbidden response with
///   the [fmt::Display] form of the variant sent as the response body.
///
///   NOTE: [401 Unauthorized] means something different in HTTP, namely that
///   the provided credentials are missing or invalid.
///
/// [401 Unauthorized]: https://datatracker.ietf.org/doc/html/rfc7235#section-3.1
#[derive(Debug, Error)]
pub enum Unauthorized {
    #[error(
        "{} is not authorized to perform action{}: {}",
        subject,
        database.map(|ident| format!(" on database {ident}")).unwrap_or_default(),
        action
    )]
    Unauthorized {
        subject: Identity,
        action: Action,
        // `Option` for future, non-database-bound actions.
        database: Option<Identity>,
        #[source]
        source: Option<anyhow::Error>,
    },
    #[error("authorization failed due to internal error")]
    InternalError(#[from] anyhow::Error),
}

impl axum::response::IntoResponse for Unauthorized {
    fn into_response(self) -> axum::response::Response {
        let (status, e) = match self {
            unauthorized @ Self::Unauthorized { .. } => (StatusCode::FORBIDDEN, anyhow!(unauthorized)),
            Self::InternalError(e) => {
                log::error!("internal error: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, e)
            }
        };

        (status, format!("{e:#}")).into_response()
    }
}

/// Action to be authorized via [Authorization::authorize_action].
#[derive(Debug)]
pub enum Action {
    CreateDatabase { parent: Option<Identity> },
    UpdateDatabase,
    ResetDatabase,
    DeleteDatabase,
    RenameDatabase,
    ViewModuleLogs,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateDatabase { parent } => match parent {
                Some(parent) => write!(f, "create database with parent {}", parent),
                None => f.write_str("create database"),
            },
            Self::UpdateDatabase => f.write_str("update database"),
            Self::ResetDatabase => f.write_str("reset database"),
            Self::DeleteDatabase => f.write_str("delete database"),
            Self::RenameDatabase => f.write_str("rename database"),
            Self::ViewModuleLogs => f.write_str("view module logs"),
        }
    }
}

/// Trait to delegate authorization of "actions" performed through the
/// client API to an external, edition-specific implementation.
pub trait Authorization {
    /// Authorize `subject` to perform [Action] `action` on `database`.
    ///
    /// Return `Ok(())` if permission is granted, `Err(Unauthorized)` if denied.
    fn authorize_action(
        &self,
        subject: Identity,
        database: Identity,
        action: Action,
    ) -> impl Future<Output = Result<(), Unauthorized>> + Send;

    /// Obtain an attenuated [AuthCtx] for `subject` to evaluate SQL against
    /// `database`.
    ///
    /// "SQL" includes the sql endpoint, pg wire connections, as well as
    /// subscription queries.
    ///
    /// If any SQL should be rejected outright, or the authorization database
    /// is not available, return `Err(Unauthorized)`.
    fn authorize_sql(
        &self,
        subject: Identity,
        database: Identity,
    ) -> impl Future<Output = Result<AuthCtx, Unauthorized>> + Send;
}

impl<T: Authorization> Authorization for Arc<T> {
    fn authorize_action(
        &self,
        subject: Identity,
        database: Identity,
        action: Action,
    ) -> impl Future<Output = Result<(), Unauthorized>> + Send {
        (**self).authorize_action(subject, database, action)
    }

    fn authorize_sql(
        &self,
        subject: Identity,
        database: Identity,
    ) -> impl Future<Output = Result<AuthCtx, Unauthorized>> + Send {
        (**self).authorize_sql(subject, database)
    }
}

pub fn log_and_500(e: impl std::fmt::Display) -> ErrorResponse {
    log::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into()
}
