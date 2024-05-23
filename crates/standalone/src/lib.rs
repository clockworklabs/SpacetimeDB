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
use scopeguard::defer_on_success;
use spacetimedb::address::Address;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::db::db_metrics;
use spacetimedb::db::{db_metrics::DB_METRICS, Config};
use spacetimedb::energy::{EnergyBalance, EnergyQuanta};
use spacetimedb::host::{HostController, ProgramStorage, UpdateDatabaseResult};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, DatabaseInstance, HostType, IdentityEmail, Node};
use spacetimedb::object_db::ObjectDb;
use spacetimedb::sendgrid_controller::SendGridController;
use spacetimedb::stdb_path;
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult, RegisterTldResult, Tld};
use spacetimedb_client_api_messages::recovery::RecoveryCode;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct StandaloneEnv {
    control_db: ControlDb,
    object_db: Arc<ObjectDb>,
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
        let object_db = ObjectDb::init().map(Arc::new)?;
        let host_controller = HostController::new(
            stdb_path("worker_node/database_instances").into(),
            config,
            ProgramStorage::external({
                let object_db = object_db.clone();
                move |hash| std::future::ready(object_db.get_object(&hash).map(|obj| obj.map(Into::into)))
            }),
            energy_monitor,
        );
        let client_actor_index = ClientActorIndex::new();
        let (public_key, private_key, public_key_bytes) = get_or_create_keys()?;

        let metrics_registry = prometheus::Registry::new();
        metrics_registry.register(Box::new(&*WORKER_METRICS)).unwrap();
        metrics_registry.register(Box::new(&*DB_METRICS)).unwrap();

        Ok(Arc::new(Self {
            control_db,
            object_db,
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

impl spacetimedb_client_api::NodeDelegate for StandaloneEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        defer_on_success! {
            db_metrics::reset_counters();
        }
        self.metrics_registry.gather()
    }

    fn host_controller(&self) -> &HostController {
        &self.host_controller
    }

    fn client_actor_index(&self) -> &ClientActorIndex {
        &self.client_actor_index
    }

    fn public_key(&self) -> &DecodingKey {
        &self.public_key
    }

    fn public_key_bytes(&self) -> &[u8] {
        &self.public_key_bytes
    }

    fn private_key(&self) -> &EncodingKey {
        &self.private_key
    }

    /// Standalone SpacetimeDB does not support SendGrid as a means to
    /// reissue authentication tokens.
    fn sendgrid_controller(&self) -> Option<&SendGridController> {
        None
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

    fn get_database_by_address(&self, address: &Address) -> anyhow::Result<Option<Database>> {
        Ok(self.control_db.get_database_by_address(address)?)
    }

    fn get_databases(&self) -> anyhow::Result<Vec<Database>> {
        Ok(self.control_db.get_databases()?)
    }

    // Database instances
    fn get_database_instance_by_id(&self, id: u64) -> anyhow::Result<Option<DatabaseInstance>> {
        Ok(self.control_db.get_database_instance_by_id(id)?)
    }

    fn get_database_instances(&self) -> anyhow::Result<Vec<DatabaseInstance>> {
        Ok(self.control_db.get_database_instances()?)
    }

    fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.control_db.get_leader_database_instance_by_database(database_id)
    }

    // Identities
    fn get_identities_for_email(&self, email: &str) -> anyhow::Result<Vec<IdentityEmail>> {
        Ok(self.control_db.get_identities_for_email(email)?)
    }

    fn get_emails_for_identity(&self, identity: &Identity) -> anyhow::Result<Vec<IdentityEmail>> {
        Ok(self.control_db.get_emails_for_identity(identity)?)
    }

    fn get_recovery_codes(&self, email: &str) -> anyhow::Result<Vec<RecoveryCode>> {
        Ok(self.control_db.spacetime_get_recovery_codes(email)?)
    }

    // Energy
    fn get_energy_balance(&self, identity: &Identity) -> anyhow::Result<Option<EnergyBalance>> {
        Ok(self.control_db.get_energy_balance(identity)?)
    }

    // DNS
    fn lookup_address(&self, domain: &DomainName) -> anyhow::Result<Option<Address>> {
        Ok(self.control_db.spacetime_dns(domain)?)
    }

    fn reverse_lookup(&self, address: &Address) -> anyhow::Result<Vec<DomainName>> {
        Ok(self.control_db.spacetime_reverse_dns(address)?)
    }
}

#[async_trait]
impl spacetimedb_client_api::ControlStateWriteAccess for StandaloneEnv {
    async fn create_address(&self) -> anyhow::Result<Address> {
        Ok(self.control_db.alloc_spacetime_address()?)
    }

    async fn publish_database(
        &self,
        identity: &Identity,
        publisher_address: Option<Address>,
        spec: spacetimedb_client_api::DatabaseDef,
    ) -> anyhow::Result<Option<UpdateDatabaseResult>> {
        let existing_db = self.control_db.get_database_by_address(&spec.address)?;
        match existing_db {
            // The database does not already exist, so we'll create it.
            None => {
                let program_bytes_address = self.object_db.insert_object(spec.program_bytes)?;
                let mut database = Database {
                    id: 0,
                    address: spec.address,
                    identity: *identity,
                    // TODO: Add to `DatabaseDef`
                    host_type: HostType::Wasm,
                    num_replicas: spec.num_replicas,
                    program_bytes_address,
                    publisher_address,
                };

                // NOTE: If initialization fails, we'll still have the database
                // in the control db.
                // This will make any subsequent access return an error about
                // the program hash not matching expectations, until the
                // database is published again.
                let database_id = self.control_db.insert_database(database.clone())?;
                database.id = database_id;

                self.schedule_database(Some(database), None).await?;

                Ok(None)
            }
            // The database already exists, so we'll try to update it.
            // If that fails, we'll keep the old one.
            Some(existing_db) => {
                ensure!(
                    &existing_db.identity == identity,
                    "Permission denied: `{}` does not own database `{}`",
                    identity,
                    spec.address.to_abbreviated_hex()
                );

                let program_bytes_address = self.object_db.insert_object(spec.program_bytes)?;
                let database = Database {
                    num_replicas: spec.num_replicas,
                    program_bytes_address,
                    publisher_address,
                    ..existing_db.clone()
                };
                let database_id = database.id;
                let database_addr = database.address;

                let leader = self
                    .control_db
                    .get_leader_database_instance_by_database(database_id)
                    .with_context(|| format!("Not found: leader instance for database `{}`", database_addr))?;
                let lock = self.lock_database_instance_for_update(leader.id)?;
                let update_result = self
                    .host_controller
                    .update_module_host(lock.token() as u128, database.clone(), leader.id)
                    .await?;

                if update_result.is_ok() {
                    self.control_db.update_database(database.clone())?;
                    self.schedule_database(Some(database), Some(existing_db)).await?;
                }

                anyhow::Ok(Some(update_result))
            }
        }
    }

    async fn delete_database(&self, identity: &Identity, address: &Address) -> anyhow::Result<()> {
        let Some(database) = self.control_db.get_database_by_address(address)? else {
            return Ok(());
        };
        anyhow::ensure!(
            &database.identity == identity,
            // TODO: `PermissionDenied` should be a variant of `Error`,
            //       so we can match on it and return better error responses
            //       from HTTP endpoints.
            "Permission denied: `{identity}` does not own database `{}`",
            address.to_abbreviated_hex()
        );

        self.control_db.delete_database(database.id)?;
        self.schedule_database(None, Some(database)).await?;

        Ok(())
    }

    async fn create_identity(&self) -> anyhow::Result<Identity> {
        Ok(self.control_db.alloc_spacetime_identity()?)
    }

    async fn add_email(&self, identity: &Identity, email: &str) -> anyhow::Result<()> {
        self.control_db
            .associate_email_spacetime_identity(*identity, email)
            .await?;
        Ok(())
    }

    async fn insert_recovery_code(&self, _identity: &Identity, email: &str, code: RecoveryCode) -> anyhow::Result<()> {
        Ok(self.control_db.spacetime_insert_recovery_code(email, code)?)
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
        identity: &Identity,
        domain: &DomainName,
        address: &Address,
    ) -> anyhow::Result<InsertDomainResult> {
        Ok(self
            .control_db
            .spacetime_insert_domain(address, domain.clone(), *identity, true)?)
    }
}

impl StandaloneEnv {
    async fn insert_database_instance(&self, database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
        let mut new_database_instance = database_instance.clone();
        let id = self.control_db.insert_database_instance(database_instance)?;
        new_database_instance.id = id;

        self.on_insert_database_instance(&new_database_instance).await?;

        Ok(())
    }

    async fn delete_database_instance(&self, database_instance_id: u64) -> Result<(), anyhow::Error> {
        self.control_db.delete_database_instance(database_instance_id)?;
        self.on_delete_database_instance(database_instance_id).await?;

        Ok(())
    }

    // Internal
    #[allow(clippy::comparison_chain)]
    async fn schedule_database(
        &self,
        database: Option<Database>,
        old_database: Option<Database>,
    ) -> Result<(), anyhow::Error> {
        let new_replicas = database.as_ref().map(|db| db.num_replicas).unwrap_or(0) as i32;
        let old_replicas = old_database.as_ref().map(|db| db.num_replicas).unwrap_or(0) as i32;
        let replica_diff = new_replicas - old_replicas;

        let database_id = if let Some(database) = database {
            database.id
        } else {
            old_database.unwrap().id
        };

        log::trace!("Scheduling database {database_id}, new_replicas {new_replicas}, old_replicas {old_replicas}");

        if replica_diff > 0 {
            self.schedule_replicas(database_id, replica_diff as u32).await?;
        } else if replica_diff < 0 {
            self.deschedule_replicas(database_id, replica_diff.unsigned_abs())
                .await?;
        }

        Ok(())
    }

    async fn schedule_replicas(&self, database_id: u64, num_replicas: u32) -> Result<(), anyhow::Error> {
        // Just scheduling a bunch of replicas to the only machine
        for i in 0..num_replicas {
            let database_instance = DatabaseInstance {
                id: 0,
                database_id,
                node_id: 0,
                leader: i == 0,
            };
            self.insert_database_instance(database_instance).await?;
        }

        Ok(())
    }

    async fn deschedule_replicas(&self, database_id: u64, num_replicas: u32) -> Result<(), anyhow::Error> {
        for _ in 0..num_replicas {
            let instances = self.control_db.get_database_instances_by_database(database_id)?;
            let Some(instance) = instances.last() else {
                return Ok(());
            };
            self.delete_database_instance(instance.id).await?;
        }
        Ok(())
    }

    async fn on_insert_database_instance(&self, instance: &DatabaseInstance) -> Result<(), anyhow::Error> {
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
            let lock = self.lock_database_instance_for_update(instance.id)?;
            self.host_controller
                .init_module_host(lock.token() as u128, database, instance.id)
                .await?;
        }

        Ok(())
    }

    async fn on_delete_database_instance(&self, instance_id: u64) -> anyhow::Result<()> {
        // TODO(cloutiertyler): We should think about how to clean up
        // database instances which have been deleted. This will just drop
        // them from memory, but will not remove them from disk.  We need
        // some kind of database lifecycle manager long term.
        self.host_controller.exit_module_host(instance_id).await?;

        Ok(())
    }

    fn lock_database_instance_for_update(&self, instance_id: u64) -> anyhow::Result<control_db::Lock> {
        let key = format!("database_instance/{}", instance_id);
        Ok(self.control_db.lock(key)?)
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
