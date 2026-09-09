mod control_db;
#[cfg(test)]
mod environment_tests;
pub mod subcommands;
pub mod util;
pub mod version;

use crate::control_db::ControlDb;
use crate::subcommands::{extract_schema, start};
use anyhow::Context as _;
use async_trait::async_trait;
use clap::{ArgMatches, Command};
use http::StatusCode;
use spacetimedb::client::ClientActorIndex;
use spacetimedb::config::{CertificateAuthority, MetadataFile, ModuleHttpConfig, V8Config, WasmConfig};
use spacetimedb::db;
use spacetimedb::db::persistence::{DurabilityConfig, LocalPersistenceProvider};
use spacetimedb::energy::{EnergyBalance, EnergyQuanta, NullEnergyMonitor};
use spacetimedb::host::{DiskStorage, HostController, HostRuntimeConfig, MigratePlanResult, UpdateDatabaseResult};
use spacetimedb::identity::{AuthCtx, Identity};
use spacetimedb::messages::control_db::{Database, HostType, Node, Replica};
use spacetimedb::metrics::ENGINE_METRICS;
use spacetimedb::subscription::row_list_builder_pool::BsatnRowListBuilderPool;
use spacetimedb::util::jobs::JobCores;
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb_client_api::auth::{self, LOCALHOST};
use spacetimedb_client_api::routes::subscribe::{HasWebSocketOptions, WebSocketOptions};
use spacetimedb_client_api::{ControlStateReadAccess, DatabaseResetDef, Host, NodeDelegate};
use spacetimedb_client_api_messages::name::{
    DatabaseName, DomainName, InsertDomainResult, RegisterTldResult, SetDomainsResult, Tld,
};
use spacetimedb_datastore::db_metrics::data_size::DATA_SIZE_METRICS;
use spacetimedb_datastore::db_metrics::DB_METRICS;
use spacetimedb_datastore::traits::Program;
use spacetimedb_paths::server::{ModuleLogsDir, PidFile, ServerDataDir};
use spacetimedb_paths::standalone::StandaloneDataDirExt;
use spacetimedb_schema::auto_migrate::{MigrationPolicy, PrettyPrintStyle};
use spacetimedb_table::page_pool::PagePool;
use std::sync::{Arc, Weak};
#[cfg(test)]
use std::time::Duration;

pub use spacetimedb_client_api::routes::subscribe::{BIN_PROTOCOL, TEXT_PROTOCOL};

#[derive(Clone, Copy)]
pub struct StandaloneOptions {
    pub db_config: db::Config,
    pub durability: DurabilityConfig,
    pub websocket: WebSocketOptions,
    pub module_http: ModuleHttpConfig,
    pub wasm: WasmConfig,
    pub v8: V8Config,
}

pub struct StandaloneEnv {
    control_db: ControlDb,
    publication_lock: Arc<tokio::sync::RwLock<()>>,
    weak_self: Weak<Self>,
    program_store: Arc<DiskStorage>,
    host_controller: HostController,
    client_actor_index: ClientActorIndex,
    metrics_registry: prometheus::Registry,
    _pid_file: PidFile,
    auth_provider: auth::DefaultJwtAuthProvider,
    websocket_options: WebSocketOptions,
}

impl StandaloneEnv {
    pub async fn init(
        config: StandaloneOptions,
        certs: &CertificateAuthority,
        data_dir: Arc<ServerDataDir>,
        db_cores: JobCores,
    ) -> anyhow::Result<Arc<Self>> {
        let _pid_file = data_dir.pid_file()?;
        let meta_path = data_dir.metadata_toml();
        let mut meta = MetadataFile::new("standalone");
        if let Some(existing_meta) = MetadataFile::read(&meta_path).context("failed reading metadata.toml")? {
            meta = existing_meta.check_compatibility_and_update(meta, meta_path.as_ref())?;
        }
        meta.write(&meta_path).context("failed writing metadata.toml")?;

        let control_db = ControlDb::new(&data_dir.control_db()).context("failed to initialize control db")?;
        let energy_monitor = Arc::new(NullEnergyMonitor);
        let program_store = Arc::new(DiskStorage::new(data_dir.program_bytes().0).await?);

        let persistence_provider =
            Arc::new(LocalPersistenceProvider::new(data_dir.clone()).with_durability_config(config.durability));
        let host_controller = HostController::new(
            data_dir,
            config.db_config,
            HostRuntimeConfig::new(config.wasm, config.v8, config.module_http),
            program_store.clone(),
            energy_monitor,
            Arc::new(()),
            persistence_provider,
            db_cores,
        )
        .with_initial_environment_source(Arc::new(control_db.clone()));
        let client_actor_index = ClientActorIndex::new();
        let jwt_keys = certs.get_or_create_keys()?;

        let auth_env = auth::default_auth_environment(jwt_keys, LOCALHOST.into());

        let metrics_registry = prometheus::Registry::new();
        metrics_registry.register(Box::new(&*WORKER_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*ENGINE_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DB_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DATA_SIZE_METRICS)).unwrap();

        Ok(Arc::new_cyclic(|weak_self| Self {
            control_db,
            publication_lock: Arc::new(tokio::sync::RwLock::new(())),
            weak_self: weak_self.clone(),
            program_store,
            host_controller,
            client_actor_index,
            metrics_registry,
            _pid_file,
            auth_provider: auth_env,
            websocket_options: config.websocket,
        }))
    }

    pub fn data_dir(&self) -> &Arc<ServerDataDir> {
        &self.host_controller.data_dir
    }

    pub fn page_pool(&self) -> &PagePool {
        &self.host_controller.page_pool
    }

    pub fn bsatn_rlb_pool(&self) -> &BsatnRowListBuilderPool {
        &self.host_controller.bsatn_rlb_pool
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GetLeaderHostError {
    #[error("database does not exist")]
    NoSuchDatabase,
    #[error("replica does not exist")]
    NoSuchReplica,
    #[error("error starting database: {source:#}")]
    LaunchError { source: anyhow::Error },
    #[error("error accessing controldb: {0:#}")]
    Control(#[from] control_db::Error),
}

impl spacetimedb_client_api::MaybeMisdirected for GetLeaderHostError {
    fn is_misdirected(&self) -> bool {
        matches!(self, Self::NoSuchDatabase | Self::NoSuchReplica)
    }
}

impl From<GetLeaderHostError> for axum::response::ErrorResponse {
    fn from(e: GetLeaderHostError) -> Self {
        let status = match e {
            GetLeaderHostError::NoSuchDatabase | GetLeaderHostError::NoSuchReplica => StatusCode::NOT_FOUND,
            GetLeaderHostError::LaunchError { .. } | GetLeaderHostError::Control { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        Self::from((status, e.to_string()))
    }
}

#[async_trait]
impl NodeDelegate for StandaloneEnv {
    type JwtAuthProviderT = auth::DefaultJwtAuthProvider;
    type GetLeaderHostError = GetLeaderHostError;

    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.metrics_registry.gather()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        &self.client_actor_index
    }

    fn jwt_auth_provider(&self) -> &Self::JwtAuthProviderT {
        &self.auth_provider
    }

    async fn leader(&self, database_id: u64) -> Result<Host, Self::GetLeaderHostError> {
        let guard = self.publication_lock.clone().read_owned().await;
        let owner = self.weak_self.upgrade().expect("standalone owner exists during lookup");
        tokio::spawn(async move {
            let _guard = guard;
            owner.leader_with_publication_lock_held(database_id).await
        })
        .await
        .map_err(|error| GetLeaderHostError::LaunchError { source: error.into() })?
    }

    fn module_logs_dir(&self, replica_id: u64) -> ModuleLogsDir {
        self.data_dir().replica(replica_id).module_logs()
    }
}

#[async_trait]
impl spacetimedb_client_api::ControlStateReadAccess for StandaloneEnv {
    // Nodes
    async fn get_node_id(&self) -> Option<u64> {
        Some(0)
    }

    async fn get_node_by_id(&self, node_id: u64) -> anyhow::Result<Option<Node>> {
        if node_id == 0 {
            return Ok(Some(Node {
                id: 0,
                unschedulable: false,
                advertise_addr: Some("node:80".to_owned()),
                pg_addr: Some("node:5432".to_owned()),
            }));
        }
        Ok(None)
    }

    async fn get_nodes(&self) -> anyhow::Result<Vec<Node>> {
        Ok(vec![self.get_node_by_id(0).await?.unwrap()])
    }

    // Databases
    async fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>> {
        Ok(self.control_db.get_database_by_id(id)?)
    }

    async fn get_database_by_identity(&self, database_identity: &Identity) -> anyhow::Result<Option<Database>> {
        Ok(self.control_db.get_database_by_identity(database_identity)?)
    }

    async fn get_databases(&self) -> anyhow::Result<Vec<Database>> {
        Ok(self.control_db.get_databases()?)
    }

    // Replicas
    async fn get_replica_by_id(&self, id: u64) -> anyhow::Result<Option<Replica>> {
        Ok(self.control_db.get_replica_by_id(id)?)
    }

    async fn get_replicas(&self) -> anyhow::Result<Vec<Replica>> {
        Ok(self.control_db.get_replicas()?)
    }

    async fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica> {
        self.control_db.get_leader_replica_by_database(database_id)
    }
    // Energy
    async fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
        Ok(self.control_db.get_energy_balance(identity)?)
    }

    // DNS
    async fn lookup_database_identity(&self, domain: &str) -> anyhow::Result<Option<Identity>> {
        Ok(self.control_db.spacetime_dns(domain)?)
    }

    async fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>> {
        Ok(self.control_db.spacetime_reverse_dns(database_identity)?)
    }

    async fn lookup_namespace_owner(&self, name: &str) -> anyhow::Result<Option<Identity>> {
        let name: DatabaseName = name.parse()?;
        Ok(self.control_db.spacetime_lookup_tld(Tld::from(name))?)
    }

    async fn is_database_locked(&self, database_identity: &Identity) -> anyhow::Result<bool> {
        Ok(self.control_db.is_database_locked(database_identity)?)
    }
}

#[async_trait]
impl spacetimedb_client_api::ControlStateWriteAccess for StandaloneEnv {
    async fn publish_database(
        &self,
        publisher: &Identity,
        spec: spacetimedb_client_api::DatabaseDef,
        policy: MigrationPolicy,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        let publisher = *publisher;
        self.own_publication(move |owner| async move { owner.publish_database_owned(&publisher, spec, policy).await })
            .await
    }

    async fn migrate_plan(
        &self,
        spec: spacetimedb_client_api::DatabaseDef,
        style: PrettyPrintStyle,
    ) -> anyhow::Result<MigratePlanResult> {
        let existing_db = self.control_db.get_database_by_identity(&spec.database_identity)?;

        match existing_db {
            Some(db) => {
                let host = self.leader(db.id).await?;
                self.host_controller
                    .migrate_plan(
                        db,
                        spec.host_type,
                        host.replica_id,
                        spec.program_bytes.to_vec().into(),
                        style,
                    )
                    .await
            }
            None => anyhow::bail!(
                "Database `{}` does not exist",
                spec.database_identity.to_abbreviated_hex()
            ),
        }
    }

    async fn delete_database(&self, _caller_identity: &Identity, database_identity: &Identity) -> anyhow::Result<()> {
        let caller_identity = *_caller_identity;
        let database_identity = *database_identity;
        self.own_publication(move |owner| async move {
            owner.delete_database_owned(&caller_identity, &database_identity).await
        })
        .await
    }

    async fn reset_database(&self, _caller_identity: &Identity, spec: DatabaseResetDef) -> anyhow::Result<()> {
        let caller_identity = *_caller_identity;
        self.own_publication(move |owner| async move { owner.reset_database_owned(&caller_identity, spec).await })
            .await
    }

    async fn add_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()> {
        let balance = self
            .control_db
            .get_energy_balance(identity)?
            .unwrap_or(EnergyBalance::ZERO);

        let balance = balance.saturating_add_energy(amount);

        self.control_db.set_energy_balance(*identity, balance)?;
        Ok(())
    }
    async fn withdraw_energy(&self, _identity: &Identity, _amount: EnergyQuanta) -> anyhow::Result<()> {
        // The energy balance code is obsolete.
        Ok(())
    }

    async fn register_tld(&self, identity: &Identity, tld: Tld) -> anyhow::Result<RegisterTldResult> {
        Ok(self.control_db.spacetime_register_tld(tld, *identity)?)
    }

    async fn create_dns_record(
        &self,
        owner_identity: &Identity,
        domain: &DomainName,
        database_identity: &Identity,
    ) -> anyhow::Result<InsertDomainResult> {
        Ok(self
            .control_db
            .spacetime_insert_domain(database_identity, domain.clone(), *owner_identity, true)?)
    }

    async fn replace_dns_records(
        &self,
        database_identity: &Identity,
        owner_identity: &Identity,
        domain_names: &[DomainName],
    ) -> anyhow::Result<SetDomainsResult> {
        Ok(self
            .control_db
            .spacetime_replace_domains(database_identity, owner_identity, domain_names)?)
    }

    async fn set_database_lock(
        &self,
        _caller_identity: &Identity,
        database_identity: &Identity,
        locked: bool,
    ) -> anyhow::Result<()> {
        let Some(_database) = self.control_db.get_database_by_identity(database_identity)? else {
            anyhow::bail!("Database not found: {}", database_identity.to_abbreviated_hex());
        };
        self.control_db.set_database_lock(database_identity, locked)?;
        Ok(())
    }
}

impl StandaloneEnv {
    /// Admission and completion are owned together. Losing an HTTP waiter cannot
    /// release the reset fence while HostController still owns accepted work.
    async fn own_publication<T, F, Fut>(&self, operation: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(Arc<Self>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        let guard = self.publication_lock.clone().write_owned().await;
        let owner = self
            .weak_self
            .upgrade()
            .expect("standalone owner exists during publication");
        tokio::spawn(async move {
            let _guard = guard;
            operation(owner).await
        })
        .await?
    }

    /// Look up or start the current leader while the caller holds the publication
    /// lock. Retain the read or write guard through this future's completion.
    /// Ordinary lookup holds a read guard; publication and reset hold a write
    /// guard, so calling `leader()` here would acquire the lock again and deadlock.
    async fn leader_with_publication_lock_held(&self, database_id: u64) -> Result<Host, GetLeaderHostError> {
        let Some(leader) = self.control_db.get_leader_replica_by_database(database_id) else {
            return Err(GetLeaderHostError::NoSuchReplica);
        };

        let Some(database) = self.control_db.get_database_by_id(database_id)? else {
            return Err(GetLeaderHostError::NoSuchDatabase);
        };

        self.host_controller
            .get_or_launch_module_host(database, leader.id)
            .await
            .map_err(|source| GetLeaderHostError::LaunchError { source })?;

        Ok(Host::new(leader.id, self.host_controller.clone()))
    }

    async fn publish_database_owned(
        &self,
        publisher: &Identity,
        spec: spacetimedb_client_api::DatabaseDef,
        policy: MigrationPolicy,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        let existing_db = self.control_db.get_database_by_identity(&spec.database_identity)?;

        // standalone does not support replication.
        let num_replicas = 1;

        match existing_db {
            // The database does not already exist, so we'll create it.
            None => {
                let program = Program::from_bytes(spec.host_type.into(), &spec.program_bytes[..]);

                let database = Database {
                    id: 0,
                    database_identity: spec.database_identity,
                    owner_identity: *publisher,
                    host_type: spec.host_type,
                    initial_program: program.hash,
                    bootstrap_generation: 0,
                };

                let _hash_for_assert = program.hash;

                // Instantiate a temporary database in order to check that the module is valid.
                // This will e.g. typecheck RLS filters.
                self.host_controller
                    .check_module_validity_with_environment(database.clone(), program, spec.environment.clone())
                    .await?;

                let program_hash = self.program_store.put(&spec.program_bytes).await?;

                debug_assert_eq!(_hash_for_assert, program_hash);

                let (database, replica) =
                    self.control_db
                        .install_database_with_environment(database, None, spec.environment, &[])?;
                // The leader nomination and input are durable already. If this
                // waiter is cancelled, ordinary lookup resumes the same input.
                self.on_insert_replica(&replica).await?;
                debug_assert_eq!(database.id, replica.database_id);

                Ok(None)
            }
            // The database already exists, so we'll try to update it.
            // If that fails, we'll keep the old one.
            Some(database) => {
                anyhow::ensure!(
                    database.owner_identity == *publisher,
                    "database ownership changed before publication"
                );
                let database_id = database.id;
                let database_identity = database.database_identity;

                let leader = self.leader_with_publication_lock_held(database_id).await?;
                let update_result = leader
                    .update_with_environment(
                        database,
                        spec.host_type,
                        spec.program_bytes.to_vec().into(),
                        policy,
                        spec.environment,
                    )
                    .await?;
                if update_result.was_successful() {
                    let replicas = self.control_db.get_replicas_by_database(database_id)?;
                    let desired_replicas = num_replicas as usize;
                    if desired_replicas == 0 {
                        log::info!("Decommissioning all replicas of database {database_identity}");
                        for instance in replicas {
                            self.delete_replica(instance.id).await?;
                        }
                    } else if desired_replicas > replicas.len() {
                        let n = desired_replicas - replicas.len();
                        log::info!(
                            "Scaling up database {} from {} to {} replicas",
                            database_identity,
                            replicas.len(),
                            n
                        );
                        for _ in 0..n {
                            self.insert_replica(Replica {
                                id: 0,
                                database_id,
                                node_id: 0,
                                leader: false,
                            })
                            .await?;
                        }
                    } else if desired_replicas < replicas.len() {
                        let n = replicas.len() - desired_replicas;
                        log::info!(
                            "Scaling down database {} from {} to {} replicas",
                            database_identity,
                            replicas.len(),
                            n
                        );
                        for instance in replicas.into_iter().filter(|instance| !instance.leader).take(n) {
                            self.delete_replica(instance.id).await?;
                        }
                    } else {
                        log::debug!(
                            "Desired replica count {desired_replicas} for database {database_identity} already satisfied"
                        );
                    }
                }

                anyhow::Ok(Some(update_result))
            }
        }
    }

    async fn delete_database_owned(
        &self,
        caller_identity: &Identity,
        database_identity: &Identity,
    ) -> anyhow::Result<()> {
        let Some(database) = self.control_db.get_database_by_identity(database_identity)? else {
            return Ok(());
        };
        anyhow::ensure!(
            database.owner_identity == *caller_identity,
            "database ownership changed before deletion"
        );
        self.control_db.delete_database(database.id)?;

        for instance in self.control_db.get_replicas_by_database(database.id)? {
            self.delete_replica(instance.id).await?;
        }

        Ok(())
    }

    async fn reset_database_owned(&self, caller_identity: &Identity, spec: DatabaseResetDef) -> anyhow::Result<()> {
        let previous = self
            .control_db
            .get_database_by_identity(&spec.database_identity)?
            .with_context(|| format!("Database `{}` does not exist", spec.database_identity))?;
        anyhow::ensure!(
            previous.owner_identity == *caller_identity,
            "database ownership changed before reset"
        );
        let mut database = previous.clone();
        let program = match spec.program_bytes {
            Some(bytes) => {
                let host_type = spec.host_type.unwrap_or(database.host_type);
                Program::from_bytes(host_type.into(), &bytes[..])
            }
            None => {
                // A reset without an artifact retains the currently committed
                // module, not the original bootstrap program or its old values.
                let module = self
                    .leader_with_publication_lock_held(database.id)
                    .await?
                    .module()
                    .await?;
                module
                    .relational_db()
                    .program()?
                    .context("database is not initialized")?
            }
        };
        database.host_type = HostType::from(program.kind);
        database.initial_program = program.hash;
        self.host_controller
            .check_module_validity_with_environment(database.clone(), program.clone(), spec.environment.clone())
            .await?;
        let stored = self.program_store.put(&program.bytes).await?;
        anyhow::ensure!(stored == program.hash, "stored reset program changed");
        let previous_replicas = self.control_db.get_replicas_by_database(database.id)?;
        // Keep old nominations until all requested closes succeed. The owned
        // publication guard excludes leader admission across close and commit.
        for replica in &previous_replicas {
            self.on_delete_replica(replica.id).await?;
        }
        let (_, replica) = self.control_db.install_database_with_environment(
            database,
            Some(&previous),
            spec.environment,
            &previous_replicas,
        )?;
        self.on_insert_replica(&replica).await?;
        Ok(())
    }
}

impl spacetimedb_client_api::Authorization for StandaloneEnv {
    async fn authorize_action(
        &self,
        subject: Identity,
        database: Identity,
        action: spacetimedb_client_api::Action,
    ) -> Result<(), spacetimedb_client_api::Unauthorized> {
        // Creating a database is always allowed.
        if let spacetimedb_client_api::Action::CreateDatabase { .. } = action {
            return Ok(());
        }

        // Otherwise, the database must already exist,
        // and the `subject` equal to `database.owner_identity`.
        let database = self
            .get_database_by_identity(&database)
            .await?
            .with_context(|| format!("database {database} not found"))
            .with_context(|| format!("Unable to authorize {subject} to perform {action:?})"))?;
        if subject == database.owner_identity {
            return Ok(());
        }

        Err(spacetimedb_client_api::Unauthorized::Unauthorized {
            subject,
            action,
            database: database.database_identity.into(),
            source: None,
        })
    }

    async fn authorize_sql(
        &self,
        subject: Identity,
        database: Identity,
    ) -> Result<AuthCtx, spacetimedb_client_api::Unauthorized> {
        let database = self
            .get_database_by_identity(&database)
            .await?
            .with_context(|| format!("database {database} not found"))
            .with_context(|| format!("Unable to authorize {subject} for SQL"))?;

        Ok(AuthCtx::new(database.owner_identity, subject))
    }
}

impl StandaloneEnv {
    async fn insert_replica(&self, replica: Replica) -> Result<(), anyhow::Error> {
        let mut new_replica = replica.clone();
        let id = self.control_db.insert_replica(replica)?;
        new_replica.id = id;

        self.on_insert_replica(&new_replica).await?;

        Ok(())
    }

    async fn delete_replica(&self, replica_id: u64) -> Result<(), anyhow::Error> {
        self.control_db.delete_replica(replica_id)?;
        self.on_delete_replica(replica_id).await?;

        Ok(())
    }

    async fn on_insert_replica(&self, instance: &Replica) -> Result<(), anyhow::Error> {
        if instance.leader {
            self.leader_with_publication_lock_held(instance.database_id)
                .await
                .with_context(|| {
                    format!(
                        "failed to start leader for database {}, replica {}",
                        instance.database_id, instance.id
                    )
                })?;
        }

        Ok(())
    }

    async fn on_delete_replica(&self, replica_id: u64) -> anyhow::Result<()> {
        // TODO(cloutiertyler): We should think about how to clean up
        // replicas which have been deleted. This will just drop
        // them from memory, but will not remove them from disk.  We need
        // some kind of database lifecycle manager long term.
        self.host_controller.exit_module_host_and_join(replica_id).await?;

        Ok(())
    }
}

impl HasWebSocketOptions for StandaloneEnv {
    fn websocket_options(&self) -> WebSocketOptions {
        self.websocket_options
    }
}

pub async fn exec_subcommand(cmd: &str, args: &ArgMatches, db_cores: JobCores) -> Result<(), anyhow::Error> {
    match cmd {
        "start" => start::exec(args, db_cores).await,
        "extract-schema" => extract_schema::exec(args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

pub fn get_subcommands() -> Vec<Command> {
    vec![start::cli(), extract_schema::cli()]
}

pub async fn start_server(data_dir: &ServerDataDir, cert_dir: Option<&std::path::Path>) -> anyhow::Result<()> {
    let mut args: Vec<&std::ffi::OsStr> = vec!["start".as_ref(), "--data-dir".as_ref(), data_dir.0.as_os_str()];
    if let Some(cert_dir) = &cert_dir {
        args.extend(["--jwt-key-dir".as_ref(), cert_dir.as_os_str()])
    }
    let args = start::cli().try_get_matches_from(args)?;
    start::exec(&args, JobCores::without_pinned_cores()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use spacetimedb::db::Storage;
    use spacetimedb_paths::{cli::*, FromPathUnchecked};
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn ensure_init_grabs_lock() -> Result<()> {
        let tempdir = TempDir::new()?;
        // Use one subdir for keys and another for the data dir.
        let keys = tempdir.path().join("keys");
        let root = tempdir.path().join("data");
        let data_dir = Arc::new(ServerDataDir::from_path_unchecked(root));

        fs::create_dir(&keys)?;
        data_dir.create()?;

        let pub_key = PubKeyPath(keys.join("public"));
        let priv_key = PrivKeyPath(keys.join("private"));
        let ca = CertificateAuthority {
            jwt_pub_key_path: pub_key,
            jwt_priv_key_path: priv_key,
        };

        // Create the keys.
        ca.get_or_create_keys()?;
        let config = StandaloneOptions {
            db_config: db::Config {
                storage: Storage::Memory,
                page_pool_max_size: None,
            },
            durability: DurabilityConfig::default(),
            websocket: WebSocketOptions::default(),
            module_http: ModuleHttpConfig::default(),
            wasm: WasmConfig::default(),
            v8: V8Config::default(),
        };

        let _env = StandaloneEnv::init(config, &ca, data_dir.clone(), JobCores::without_pinned_cores()).await?;
        // Ensure that we have a lock.
        assert!(
            StandaloneEnv::init(config, &ca, data_dir.clone(), JobCores::without_pinned_cores())
                .await
                .is_err()
        );

        Ok(())
    }
    #[tokio::test]
    async fn cancelled_publication_waiter_keeps_mutations_and_leader_admission_fenced() -> Result<()> {
        let tempdir = TempDir::new()?;
        // Use one subdir for keys and another for the data dir.
        let keys = tempdir.path().join("keys");
        let root = tempdir.path().join("data");
        let data_dir = Arc::new(ServerDataDir::from_path_unchecked(root));

        fs::create_dir(&keys)?;
        data_dir.create()?;

        let pub_key = PubKeyPath(keys.join("public"));
        let priv_key = PrivKeyPath(keys.join("private"));
        let ca = CertificateAuthority {
            jwt_pub_key_path: pub_key,
            jwt_priv_key_path: priv_key,
        };

        // Create the keys.
        ca.get_or_create_keys()?;
        let config = StandaloneOptions {
            db_config: db::Config {
                storage: Storage::Memory,
                page_pool_max_size: None,
            },
            durability: DurabilityConfig::default(),
            websocket: WebSocketOptions::default(),
            module_http: ModuleHttpConfig::default(),
            wasm: WasmConfig::default(),
            v8: V8Config::default(),
        };

        let env = StandaloneEnv::init(config, &ca, data_dir.clone(), JobCores::without_pinned_cores()).await?;

        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel();
        let owner = env.clone();
        let waiter = tokio::spawn(async move {
            owner
                .own_publication(move |_owner| async move {
                    let _ = started.send(());
                    release_rx.await?;
                    Ok(())
                })
                .await
        });
        started_rx.await?;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());

        // Both a later mutation and ordinary leader lookup must stay behind the
        // accepted operation even after its request future has disappeared.
        let next_owner = env.clone();
        let mut next = tokio::spawn(async move { next_owner.own_publication(|_| async { Ok(()) }).await });
        assert!(tokio::time::timeout(Duration::from_millis(30), &mut next)
            .await
            .is_err());
        let read_owner = env.clone();
        let mut reader = tokio::spawn(async move { read_owner.leader(u64::MAX).await });
        assert!(tokio::time::timeout(Duration::from_millis(30), &mut reader)
            .await
            .is_err());
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), next).await???;
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), reader).await??,
            Err(GetLeaderHostError::NoSuchReplica)
        ));
        Ok(())
    }
}
