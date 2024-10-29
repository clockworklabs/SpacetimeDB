mod control_db;
mod energy_monitor;
pub mod routes;
pub mod subcommands;
pub mod util;

use crate::control_db::ControlDb;
use crate::subcommands::start::ProgramMode;
use crate::subcommands::{start, version};
use anyhow::{ensure, Context};
use async_trait::async_trait;
use clap::{ArgMatches, Command};
use energy_monitor::StandaloneEnergyMonitor;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::auth::JwtKeys;
use spacetimedb::client::ClientActorIndex;
use spacetimedb::config::{CertificateAuthority, MetadataFile};
use spacetimedb::db::{db_metrics::DB_METRICS, Config};
use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
use spacetimedb::host::{DiskStorage, HostController, UpdateDatabaseResult};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, Node, Replica};
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb_client_api::auth::LOCALHOST;
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, Tld};
use spacetimedb_paths::server::{PidFile, ServerDataDir};
use spacetimedb_paths::standalone::StandaloneDataDirExt;
use std::sync::Arc;

pub use spacetimedb_client_api::routes::subscribe::{BIN_PROTOCOL, TEXT_PROTOCOL};

pub struct StandaloneEnv {
    control_db: ControlDb,
    program_store: Arc<DiskStorage>,
    host_controller: HostController,
    client_actor_index: ClientActorIndex,
    jwt_keys: JwtKeys,
    metrics_registry: prometheus::Registry,
    _pid_file: PidFile,
}

impl StandaloneEnv {
    pub async fn init(
        config: Config,
        certs: &CertificateAuthority,
        data_dir: Arc<ServerDataDir>,
    ) -> anyhow::Result<Arc<Self>> {
        let _pid_file = data_dir.pid_file()?;
        let meta_path = data_dir.metadata_toml();
        let meta = MetadataFile {
            version: spacetimedb::config::current_version(),
            edition: "standalone".to_owned(),
            client_address: None,
        };
        if let Some(existing_meta) = MetadataFile::read(&meta_path).context("failed reading metadata.toml")? {
            anyhow::ensure!(
                existing_meta.version_compatible_with(&meta.version) && existing_meta.edition == meta.edition,
                "metadata.toml indicates that this database is from an incompatible \
                 version of SpacetimeDB. please run a migration before proceeding."
            );
        }
        meta.write(&meta_path).context("failed writing metadata.toml")?;

        let control_db = ControlDb::new(&data_dir.control_db()).context("failed to initialize control db")?;
        let energy_monitor = Arc::new(StandaloneEnergyMonitor::new(control_db.clone()));
        let program_store = Arc::new(DiskStorage::new(data_dir.program_bytes().0).await?);

        let host_controller = HostController::new(data_dir, config, program_store.clone(), energy_monitor);
        let client_actor_index = ClientActorIndex::new();
        let jwt_keys = certs.get_or_create_keys()?;

        let metrics_registry = prometheus::Registry::new();
        metrics_registry.register(Box::new(&*WORKER_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DB_METRICS)).unwrap();

        Ok(Arc::new(Self {
            control_db,
            program_store,
            host_controller,
            client_actor_index,
            jwt_keys,
            metrics_registry,
            _pid_file,
        }))
    }

    pub fn data_dir(&self) -> &Arc<ServerDataDir> {
        &self.host_controller.data_dir
    }
}

impl spacetimedb_client_api::NodeDelegate for StandaloneEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.metrics_registry.gather()
    }

    fn host_controller(&self) -> &HostController {
        &self.host_controller
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        &self.client_actor_index
    }

    fn public_key(&self) -> &DecodingKey {
        &self.jwt_keys.public
    }

    fn local_issuer(&self) -> String {
        LOCALHOST.to_owned()
    }

    fn public_key_bytes(&self) -> &[u8] {
        &self.jwt_keys.public_pem
    }

    fn private_key(&self) -> &EncodingKey {
        &self.jwt_keys.private
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
    fn lookup_identity(&self, domain: &DomainName) -> anyhow::Result<Option<Identity>> {
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

        match existing_db {
            // The database does not already exist, so we'll create it.
            None => {
                let initial_program = self.program_store.put(&spec.program_bytes).await?;
                let mut database = Database {
                    id: 0,
                    database_identity: spec.database_identity,
                    owner_identity: *publisher,
                    host_type: spec.host_type,
                    initial_program,
                };
                let database_id = self.control_db.insert_database(database.clone())?;
                database.id = database_id;

                self.schedule_replicas(database_id, spec.num_replicas).await?;

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
                    .control_db
                    .get_leader_replica_by_database(database_id)
                    .with_context(|| format!("Not found: leader instance for database `{}`", database_identity))?;
                let update_result = self
                    .host_controller
                    .update_module_host(database, spec.host_type, leader.id, spec.program_bytes.into())
                    .await?;

                if update_result.was_successful() {
                    let replicas = self.control_db.get_replicas_by_database(database_id)?;
                    let desired_replicas = spec.num_replicas as usize;
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
    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()> {
        withdraw_energy(&self.control_db, identity, amount)
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

    async fn schedule_replicas(&self, database_id: u64, num_replicas: u32) -> Result<(), anyhow::Error> {
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
            self.host_controller
                .get_or_launch_module_host(database, instance.id)
                .await
                .map(drop)?
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

fn withdraw_energy(control_db: &ControlDb, identity: &Identity, amount: EnergyQuanta) -> anyhow::Result<()> {
    let energy_balance = control_db.get_energy_balance(identity)?;
    let energy_balance = energy_balance.unwrap_or(EnergyBalance::ZERO);
    log::trace!("Withdrawing {} from {}", amount, identity);
    log::trace!("Old balance: {}", energy_balance);
    let new_balance = energy_balance.saturating_sub_energy(amount);
    control_db.set_energy_balance(*identity, new_balance)?;
    Ok(())
}

pub async fn exec_subcommand(cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "start" => start::exec(None, args).await,
        "version" => version::exec(args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub fn get_subcommands() -> Vec<Command> {
    vec![start::cli(ProgramMode::Standalone), version::cli()]
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use spacetimedb::db::Storage;
    use spacetimedb_paths::cli::*;
    use tempfile::TempDir;
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn ensure_init_grabs_lock() -> Result<()> {
        let tempdir = TempDir::new()?;
        // Use one subdir for keys and another for the data dir.
        let keys = tempdir.path().join("keys");
        let root = tempdir.path().join("root");

        fs::create_dir(&keys)?;
        fs::create_dir(&root)?;

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
        };
        
        let _env = StandaloneEnv::init(config, &ca, Arc::new(ServerDataDir(root.clone()))).await?;
        // Ensure that we have a lock.
        assert!(StandaloneEnv::init(config, &ca, Arc::new(ServerDataDir(root))).await.is_err());

        Ok(())

    }
}