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
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::db::relational_db;
use spacetimedb::db::{db_metrics::DB_METRICS, Config};
use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
use spacetimedb::host::{DiskStorage, DynDurabilityFut, HostController, UpdateDatabaseResult};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, Node, Replica};
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb::{db, stdb_path};
use spacetimedb_client_api::auth::LOCALHOST;
use spacetimedb_client_api::{Host, NodeDelegate};
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, Tld};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use spacetimedb_client_api::routes::subscribe::{BIN_PROTOCOL, TEXT_PROTOCOL};

pub struct StandaloneEnv {
    control_db: ControlDb,
    program_store: Arc<DiskStorage>,
    host_controller: HostController,
    client_actor_index: ClientActorIndex,
    public_key: DecodingKey,
    private_key: EncodingKey,
    public_key_bytes: Box<[u8]>,
    metrics_registry: prometheus::Registry,
}

impl StandaloneEnv {
    pub async fn init(config: Config) -> anyhow::Result<Arc<Self>> {
        let control_db = ControlDb::new().context("failed to initialize control db")?;
        let energy_monitor = Arc::new(StandaloneEnergyMonitor::new(control_db.clone()));
        let program_store = Arc::new(DiskStorage::new(stdb_path("control_node/program_bytes")).await?);

        let host_controller = HostController::new(
            stdb_path("worker_node/replicas").into(),
            config,
            program_store.clone(),
            energy_monitor,
        );
        let client_actor_index = ClientActorIndex::new();
        let (public_key, private_key, public_key_bytes) = get_or_create_keys()?;

        let metrics_registry = prometheus::Registry::new();
        metrics_registry.register(Box::new(&*WORKER_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DB_METRICS)).unwrap();

        Ok(Arc::new(Self {
            control_db,
            program_store,
            host_controller,
            client_actor_index,
            public_key,
            private_key,
            public_key_bytes,
            metrics_registry,
        }))
    }
}

fn get_or_create_keys() -> anyhow::Result<(DecodingKey, EncodingKey, Box<[u8]>)> {
    let public_key_path =
        get_key_path("SPACETIMEDB_JWT_PUB_KEY").expect("SPACETIMEDB_JWT_PUB_KEY must be set to a valid path");
    let private_key_path =
        get_key_path("SPACETIMEDB_JWT_PRIV_KEY").expect("SPACETIMEDB_JWT_PRIV_KEY must be set to a valid path");

    let mut public_key_bytes = read_key(&public_key_path).ok();
    let mut private_key_bytes = read_key(&private_key_path).ok();

    // If both keys are unspecified, create them
    if public_key_bytes.is_none() && private_key_bytes.is_none() {
        create_keys(&public_key_path, &private_key_path)?;
        public_key_bytes = Some(read_key(&public_key_path)?);
        private_key_bytes = Some(read_key(&private_key_path)?);
    }

    if public_key_bytes.is_none() {
        anyhow::bail!("Unable to read public key for JWT token verification");
    }

    if private_key_bytes.is_none() {
        anyhow::bail!("Unable to read private key for JWT token signing");
    }

    let public_key_bytes = Box::<[u8]>::from(public_key_bytes.unwrap());

    let encoding_key = EncodingKey::from_ec_pem(&private_key_bytes.unwrap())?;
    let decoding_key = DecodingKey::from_ec_pem(&public_key_bytes)?;

    Ok((decoding_key, encoding_key, public_key_bytes))
}

fn read_key(path: &Path) -> anyhow::Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("couldn't read key from {path:?}"))
}

fn create_keys(public_key_path: &Path, private_key_path: &Path) -> anyhow::Result<()> {
    // Create a new EC group from a named curve.
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;

    // Create a new EC key with the specified group.
    let eckey = EcKey::generate(&group)?;

    // Create a new PKey from the EC key.
    let pkey = PKey::from_ec_key(eckey.clone())?;

    // Get the private key in PKCS#8 PEM format.
    let private_key = pkey.private_key_to_pem_pkcs8()?;

    // Write the private key to a file.
    if let Some(parent) = private_key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut priv_file = File::create(private_key_path)?;
    priv_file.write_all(&private_key)?;

    // Get the public key in PEM format.
    let public_key = eckey.public_key_to_pem()?;

    // Write the public key to a file.
    if let Some(parent) = public_key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut pub_file = File::create(public_key_path)?;
    pub_file.write_all(&public_key)?;
    Ok(())
}

fn get_key_path(env: &str) -> Option<PathBuf> {
    std::env::var_os(env).map(std::path::PathBuf::from)
}

#[async_trait]
impl NodeDelegate for StandaloneEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.metrics_registry.gather()
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        &self.client_actor_index
    }

    fn public_key(&self) -> &DecodingKey {
        &self.public_key
    }

    fn local_issuer(&self) -> String {
        LOCALHOST.to_owned()
    }

    fn public_key_bytes(&self) -> &[u8] {
        &self.public_key_bytes
    }

    fn private_key(&self) -> &EncodingKey {
        &self.private_key
    }

    async fn leader(&self, database_id: u64) -> anyhow::Result<Option<Host>> {
        let leader = match self.control_db.get_leader_replica_by_database(database_id) {
            Some(leader) => leader,
            None => return Ok(None),
        };

        let database = self
            .control_db
            .get_database_by_id(database_id)?
            .ok_or_else(|| anyhow::anyhow!("Database {} has no associated database", database_id))?;

        let durability = durability(self.host_controller.clone(), &database.database_identity, leader.id);
        self.host_controller
            .get_or_launch_module_host(database, leader.id, durability)
            .await
            .context("failed to get or launch module host")?;

        Ok(Some(Host::new(leader.id, self.host_controller.clone())))
    }
}

pub fn durability(ctrl: HostController, address: &Identity, replica_id: u64) -> DynDurabilityFut {
    match ctrl.get_config().storage {
        db::Storage::Disk => {
            let db_path = ctrl.get_replica_path(&address, replica_id);
            Box::pin(async { relational_db::local_durability_dyn(db_path).await })
        }
        db::Storage::Memory => {
            let err_fut = async { Err(anyhow::anyhow!("Memory storage not supported for durability")) };
            Box::pin(err_fut)
        }
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

                let num_replicas = spec.num_replicas;
                let leader = self
                    .leader(database_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No leader for database"))?;
                let durability = durability(self.host_controller.clone(), &database_identity, leader.replica_id);
                let update_result = leader
                    .update(database, spec.host_type, spec.program_bytes.into(), durability)
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
        "start" => start::exec(args).await,
        "version" => version::exec(args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub fn get_subcommands() -> Vec<Command> {
    vec![start::cli(ProgramMode::Standalone), version::cli()]
}
