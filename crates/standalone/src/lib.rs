mod control_db;
pub mod subcommands;
pub mod util;
pub mod version;

use crate::control_db::ControlDb;
use crate::subcommands::{extract_schema, start};
use anyhow::{ensure, Context, Ok};
use async_trait::async_trait;
use clap::{ArgMatches, Command};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::config::{CertificateAuthority, MetadataFile};
use spacetimedb::db::datastore::traits::Program;
use spacetimedb::db::db_metrics::data_size::DATA_SIZE_METRICS;
use spacetimedb::db::relational_db;
use spacetimedb::db::{db_metrics::DB_METRICS, Config};
use spacetimedb::energy::{EnergyBalance, EnergyQuanta, NullEnergyMonitor};
use spacetimedb::host::{
    DiskStorage, DurabilityProvider, ExternalDurability, HostController, StartSnapshotWatcher, UpdateDatabaseResult,
};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, Node, Replica};
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb_client_api::auth::{self, LOCALHOST};
use spacetimedb_client_api::{Host, NodeDelegate};
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, SetDomainsResult, Tld};
use spacetimedb_paths::server::{ModuleLogsDir, PidFile, ServerDataDir};
use spacetimedb_paths::standalone::StandaloneDataDirExt;
use spacetimedb_table::page_pool::PagePool;
use std::sync::Arc;

pub use spacetimedb_client_api::routes::subscribe::{BIN_PROTOCOL, TEXT_PROTOCOL};

pub struct StandaloneEnv {
    control_db: ControlDb,
    program_store: Arc<DiskStorage>,
    host_controller: HostController,
    client_actor_index: ClientActorIndex,
    metrics_registry: prometheus::Registry,
    _pid_file: PidFile,
    auth_provider: auth::DefaultJwtAuthProvider,
}

impl StandaloneEnv {
    pub async fn init(
        config: Config,
        certs: &CertificateAuthority,
        data_dir: Arc<ServerDataDir>,
    ) -> anyhow::Result<Arc<Self>> {
        let _pid_file = data_dir.pid_file()?;
        let meta_path = data_dir.metadata_toml();
        let mut meta = MetadataFile::new("standalone");
        if let Some(existing_meta) = MetadataFile::read(&meta_path).context("failed reading metadata.toml")? {
            meta = existing_meta.check_compatibility_and_update(meta)?;
        }
        meta.write(&meta_path).context("failed writing metadata.toml")?;

        let control_db = ControlDb::new(&data_dir.control_db()).context("failed to initialize control db")?;
        let energy_monitor = Arc::new(NullEnergyMonitor);
        let program_store = Arc::new(DiskStorage::new(data_dir.program_bytes().0).await?);

        let durability_provider = Arc::new(StandaloneDurabilityProvider {
            data_dir: data_dir.clone(),
        });
        let host_controller = HostController::new(
            data_dir,
            config,
            program_store.clone(),
            energy_monitor,
            durability_provider,
        );
        let client_actor_index = ClientActorIndex::new();
        let jwt_keys = certs.get_or_create_keys()?;

        let auth_env = auth::default_auth_environment(jwt_keys, LOCALHOST.to_owned());

        let metrics_registry = prometheus::Registry::new();
        metrics_registry.register(Box::new(&*WORKER_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DB_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DATA_SIZE_METRICS)).unwrap();

        Ok(Arc::new(Self {
            control_db,
            program_store,
            host_controller,
            client_actor_index,
            metrics_registry,
            _pid_file,
            auth_provider: auth_env,
        }))
    }

    pub fn data_dir(&self) -> &Arc<ServerDataDir> {
        &self.host_controller.data_dir
    }

    pub fn page_pool(&self) -> &PagePool {
        &self.host_controller.page_pool
    }
}

struct StandaloneDurabilityProvider {
    data_dir: Arc<ServerDataDir>,
}

#[async_trait]
impl DurabilityProvider for StandaloneDurabilityProvider {
    async fn durability(&self, replica_id: u64) -> anyhow::Result<(ExternalDurability, Option<StartSnapshotWatcher>)> {
        let commitlog_dir = self.data_dir.replica(replica_id).commit_log();
        let (durability, disk_size) = relational_db::local_durability(commitlog_dir).await?;
        let start_snapshot_watcher = {
            let durability = durability.clone();
            |snapshot_rx| {
                tokio::spawn(relational_db::snapshot_watching_commitlog_compressor(
                    snapshot_rx,
                    durability,
                ));
            }
        };
        Ok(((durability, disk_size), Some(Box::new(start_snapshot_watcher))))
    }
}

#[async_trait]
impl NodeDelegate for StandaloneEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.metrics_registry.gather()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        &self.client_actor_index
    }

    type JwtAuthProviderT = auth::DefaultJwtAuthProvider;

    fn jwt_auth_provider(&self) -> &Self::JwtAuthProviderT {
        &self.auth_provider
    }

    async fn leader(&self, database_id: u64) -> anyhow::Result<Option<Host>> {
        let leader = match self.control_db.get_leader_replica_by_database(database_id) {
            Some(leader) => leader,
            None => return Ok(None),
        };

        let database = self
            .control_db
            .get_database_by_id(database_id)?
            .with_context(|| format!("Database {} not found", database_id))?;

        self.host_controller
            .get_or_launch_module_host(database, leader.id)
            .await
            .context("failed to get or launch module host")?;

        Ok(Some(Host::new(leader.id, self.host_controller.clone())))
    }
    fn module_logs_dir(&self, replica_id: u64) -> ModuleLogsDir {
        self.data_dir().replica(replica_id).module_logs()
    }
}

impl spacetimedb_client_api::ControlStateReadAccess for StandaloneEnv {
    // Nodes
    fn get_node_id(&self) -> Option<u64> {
        Some(0)
    }

    fn get_node_by_id(&self, node_id: u64) -> anyhow::Result<Option<Node>> {
        if node_id == 0 {
            return Ok(Some(Node {
                id: 0,
                unschedulable: false,
                advertise_addr: Some("node:80".to_owned()),
            }));
        }
        Ok(None)
    }

    fn get_nodes(&self) -> anyhow::Result<Vec<Node>> {
        Ok(vec![self.get_node_by_id(0)?.unwrap()])
    }

    // Databases
    fn get_database_by_id(&self, id: u64) -> anyhow::Result<Option<Database>> {
        Ok(self.control_db.get_database_by_id(id)?)
    }

    fn get_database_by_identity(&self, database_identity: &Identity) -> anyhow::Result<Option<Database>> {
        Ok(self.control_db.get_database_by_identity(database_identity)?)
    }

    fn get_databases(&self) -> anyhow::Result<Vec<Database>> {
        Ok(self.control_db.get_databases()?)
    }

    // Replicas
    fn get_replica_by_id(&self, id: u64) -> anyhow::Result<Option<Replica>> {
        Ok(self.control_db.get_replica_by_id(id)?)
    }

    fn get_replicas(&self) -> anyhow::Result<Vec<Replica>> {
        Ok(self.control_db.get_replicas()?)
    }

    fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica> {
        self.control_db.get_leader_replica_by_database(database_id)
    }
    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
        Ok(self.control_db.get_energy_balance(identity)?)
    }

    // DNS
    fn lookup_identity(&self, domain: &str) -> anyhow::Result<Option<Identity>> {
        Ok(self.control_db.spacetime_dns(domain)?)
    }

    fn reverse_lookup(&self, database_identity: &Identity) -> anyhow::Result<Vec<DomainName>> {
        Ok(self.control_db.spacetime_reverse_dns(database_identity)?)
    }
}

#[async_trait]
impl spacetimedb_client_api::ControlStateWriteAccess for StandaloneEnv {
    async fn publish_database(
        &self,
        publisher: &Identity,
        spec: spacetimedb_client_api::DatabaseDef,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        let existing_db = self.control_db.get_database_by_identity(&spec.database_identity)?;

        // standalone does not support replication.
        let num_replicas = 1;

        match existing_db {
            // The database does not already exist, so we'll create it.
            None => {
                let program = Program::from_bytes(&spec.program_bytes[..]);

                let mut database = Database {
                    id: 0,
                    database_identity: spec.database_identity,
                    owner_identity: *publisher,
                    host_type: spec.host_type,
                    initial_program: program.hash,
                };

                let _hash_for_assert = program.hash;

                // Instantiate a temporary database in order to check that the module is valid.
                // This will e.g. typecheck RLS filters.
                self.host_controller
                    .check_module_validity(database.clone(), program)
                    .await?;

                let program_hash = self.program_store.put(&spec.program_bytes).await?;

                debug_assert_eq!(_hash_for_assert, program_hash);

                let database_id = self.control_db.insert_database(database.clone())?;
                database.id = database_id;

                self.schedule_replicas(database_id, num_replicas).await?;

                Ok(None)
            }
            // The database already exists, so we'll try to update it.
            // If that fails, we'll keep the old one.
            Some(database) => {
                ensure!(
                    &database.owner_identity == publisher,
                    "Permission denied: `{}` does not own database `{}`",
                    publisher,
                    spec.database_identity.to_abbreviated_hex()
                );

                let database_id = database.id;
                let database_identity = database.database_identity;

                let leader = self
                    .leader(database_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No leader for database"))?;
                let update_result = leader
                    .update(database, spec.host_type, spec.program_bytes.into())
                    .await?;
                if update_result.was_successful() {
                    let replicas = self.control_db.get_replicas_by_database(database_id)?;
                    let desired_replicas = num_replicas as usize;
                    if desired_replicas == 0 {
                        log::info!("Decommissioning all replicas of database {}", database_identity);
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
                            "Desired replica count {} for database {} already satisfied",
                            desired_replicas,
                            database_identity
                        );
                    }
                }

                anyhow::Ok(Some(update_result))
            }
        }
    }

    async fn delete_database(&self, caller_identity: &Identity, database_identity: &Identity) -> anyhow::Result<()> {
        let Some(database) = self.control_db.get_database_by_identity(database_identity)? else {
            return Ok(());
        };
        anyhow::ensure!(
            &database.owner_identity == caller_identity,
            // TODO: `PermissionDenied` should be a variant of `Error`,
            //       so we can match on it and return better error responses
            //       from HTTP endpoints.
            "Permission denied: `{caller_identity}` does not own database `{}`",
            database_identity.to_abbreviated_hex()
        );

        self.control_db.delete_database(database.id)?;

        for instance in self.control_db.get_replicas_by_database(database.id)? {
            self.delete_replica(instance.id).await?;
        }

        Ok(())
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

    async fn schedule_replicas(&self, database_id: u64, num_replicas: u8) -> Result<(), anyhow::Error> {
        // Just scheduling a bunch of replicas to the only machine
        for i in 0..num_replicas {
            let replica = Replica {
                id: 0,
                database_id,
                node_id: 0,
                leader: i == 0,
            };
            self.insert_replica(replica).await?;
        }

        Ok(())
    }

    async fn on_insert_replica(&self, instance: &Replica) -> Result<(), anyhow::Error> {
        if instance.leader {
            let database = self
                .control_db
                .get_database_by_id(instance.database_id)?
                .with_context(|| {
                    format!(
                        "unknown database: id: {}, instance: {}",
                        instance.database_id, instance.id
                    )
                })?;
            self.leader(database.id).await?;
        }

        Ok(())
    }

    async fn on_delete_replica(&self, replica_id: u64) -> anyhow::Result<()> {
        // TODO(cloutiertyler): We should think about how to clean up
        // replicas which have been deleted. This will just drop
        // them from memory, but will not remove them from disk.  We need
        // some kind of database lifecycle manager long term.
        self.host_controller.exit_module_host(replica_id).await?;

        Ok(())
    }
}

pub async fn exec_subcommand(cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "start" => start::exec(args).await,
        "extract-schema" => extract_schema::exec(args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
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
    start::exec(&args).await
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
        let config = Config {
            storage: Storage::Memory,
            page_pool_max_size: None,
        };

        let _env = StandaloneEnv::init(config, &ca, data_dir.clone()).await?;
        // Ensure that we have a lock.
        assert!(StandaloneEnv::init(config, &ca, data_dir.clone()).await.is_err());

        Ok(())
    }
}
