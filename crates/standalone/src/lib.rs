mod energy_monitor;
pub mod routes;
pub mod subcommands;
pub mod util;
mod worker_db;

use crate::subcommands::start::ProgramMode;
use crate::subcommands::{start, version};
use anyhow::Context;
use clap::{ArgMatches, Command};
use energy_monitor::StandaloneEnergyMonitor;
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use spacetimedb::address::Address;
use spacetimedb::auth::identity::{DecodingKey, EncodingKey};
use spacetimedb::client::ClientActorIndex;
use spacetimedb::control_db::ControlDb;
use spacetimedb::database_instance_context::DatabaseInstanceContext;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::db::{db_metrics, Storage};
use spacetimedb::hash::Hash;
use spacetimedb::host::UpdateOutcome;
use spacetimedb::host::{scheduler::Scheduler, HostController};
use spacetimedb::host::{EnergyQuanta, UpdateDatabaseResult};
use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, DatabaseInstance, HostType, Node};
use spacetimedb::messages::worker_db::DatabaseInstanceState;
use spacetimedb::module_host_context::ModuleHostContext;
use spacetimedb::object_db::ObjectDb;
use spacetimedb::sendgrid_controller::SendGridController;
use spacetimedb::{stdb_path, worker_metrics};
use spacetimedb_lib::name::DomainName;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use worker_db::WorkerDb;

pub struct StandaloneEnv {
    worker_db: WorkerDb,
    control_db: ControlDb,
    db_inst_ctx_controller: DatabaseInstanceContextController,
    object_db: ObjectDb,
    host_controller: Arc<HostController>,
    client_actor_index: ClientActorIndex,
    public_key: DecodingKey,
    private_key: EncodingKey,

    /// Whether databases in this environment will be created entirely in memory
    /// or otherwise persist their message log and object store to disk.
    ///
    /// Note that this does not apply to the StandaloneEnv's own control_db
    /// or object_db.
    storage: Storage,
}

impl StandaloneEnv {
    pub async fn init(storage: Storage) -> anyhow::Result<Arc<Self>> {
        let worker_db = WorkerDb::init()?;
        let object_db = ObjectDb::init()?;
        let db_inst_ctx_controller = DatabaseInstanceContextController::new();
        let control_db = ControlDb::new()?;
        let energy_monitor = Arc::new(StandaloneEnergyMonitor::new());
        let host_controller = Arc::new(HostController::new(energy_monitor.clone()));
        let client_actor_index = ClientActorIndex::new();
        let (public_key, private_key) = get_or_create_keys()?;
        let this = Arc::new(Self {
            worker_db,
            control_db,
            db_inst_ctx_controller,
            object_db,
            host_controller,
            client_actor_index,
            public_key,
            private_key,
            storage,
        });
        energy_monitor.set_standalone_env(this.clone());
        Ok(this)
    }
}

fn get_or_create_keys() -> anyhow::Result<(DecodingKey, EncodingKey)> {
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

    let encoding_key = EncodingKey::from_ec_pem(&private_key_bytes.unwrap())?;
    let decoding_key = DecodingKey::from_ec_pem(&public_key_bytes.unwrap())?;

    Ok((decoding_key, encoding_key))
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
    let Some(path) = std::env::var_os(env) else {
        return None;
    };
    let path = std::path::PathBuf::from(path);
    Some(path)
}

#[async_trait::async_trait]
impl spacetimedb_client_api::WorkerCtx for StandaloneEnv {
    fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        let mut metric_families = worker_metrics::REGISTRY.gather();
        metric_families.extend(db_metrics::REGISTRY.gather());
        metric_families
    }

    fn database_instance_context_controller(&self) -> &DatabaseInstanceContextController {
        &self.db_inst_ctx_controller
    }

    async fn load_module_host_context(&self, db: Database, instance_id: u64) -> anyhow::Result<ModuleHostContext> {
        self.load_module_host_context_inner(db, instance_id).await
    }

    fn host_controller(&self) -> &Arc<HostController> {
        &self.host_controller
    }
    fn client_actor_index(&self) -> &ClientActorIndex {
        &self.client_actor_index
    }
}

#[async_trait::async_trait]
impl spacetimedb_client_api::ControlStateDelegate for StandaloneEnv {
    async fn get_node_id(&self) -> Result<Option<u64>, anyhow::Error> {
        Ok(Some(0))
    }

    async fn get_node_by_id(&self, node_id: u64) -> spacetimedb::control_db::Result<Option<Node>> {
        if node_id == 0 {
            return Ok(Some(Node {
                id: 0,
                unschedulable: false,
                advertise_addr: "node:80".into(),
            }));
        }
        Ok(None)
    }

    async fn get_nodes(&self) -> spacetimedb::control_db::Result<Vec<Node>> {
        Ok(vec![self.get_node_by_id(0).await?.unwrap()])
    }

    async fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> Result<Option<DatabaseInstanceState>, anyhow::Error> {
        self.worker_db.get_database_instance_state(database_instance_id)
    }

    async fn get_database_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<Database>> {
        self.control_db.get_database_by_id(id).await
    }

    async fn get_database_by_address(&self, address: &Address) -> spacetimedb::control_db::Result<Option<Database>> {
        self.control_db.get_database_by_address(address).await
    }

    async fn get_databases(&self) -> spacetimedb::control_db::Result<Vec<Database>> {
        self.control_db.get_databases().await
    }

    async fn get_database_instance_by_id(&self, id: u64) -> spacetimedb::control_db::Result<Option<DatabaseInstance>> {
        self.control_db.get_database_instance_by_id(id).await
    }

    async fn get_database_instances(&self) -> spacetimedb::control_db::Result<Vec<DatabaseInstance>> {
        self.control_db.get_database_instances().await
    }

    async fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.control_db
            .get_leader_database_instance_by_database(database_id)
            .await
    }
}

#[async_trait::async_trait]
impl spacetimedb_client_api::ControlCtx for StandaloneEnv {
    async fn insert_database(
        &self,
        address: &Address,
        identity: &Identity,
        program_bytes_address: &Hash,
        host_type: HostType,
        num_replicas: u32,
        force: bool,
    ) -> Result<(), anyhow::Error> {
        let database = Database {
            id: 0,
            address: *address,
            identity: *identity,
            host_type,
            num_replicas,
            program_bytes_address: *program_bytes_address,
        };

        if force {
            self.delete_database(address).await?;
        }

        let mut new_database = database.clone();
        let id = self.control_db.insert_database(database).await?;
        new_database.id = id;

        self.schedule_database(Some(new_database), None).await?;
        Ok(())
    }

    async fn update_database(
        &self,
        address: &Address,
        program_bytes_address: &Hash,
        num_replicas: u32,
    ) -> Result<Option<UpdateDatabaseResult>, anyhow::Error> {
        let database = self.control_db.get_database_by_address(address).await?;
        let mut database = match database {
            Some(database) => database,
            None => return Ok(None),
        };

        let old_database = database.clone();

        database.program_bytes_address = *program_bytes_address;
        database.num_replicas = num_replicas;
        let database_id = database.id;
        let new_database = database.clone();
        self.control_db.update_database(database).await?;

        self.schedule_database(Some(new_database), Some(old_database)).await?;
        self.update_database_instances(database_id)
            .await
            // TODO(kim): this should really only run on the leader instance
            .map(|mut res| res.pop().flatten())
    }

    async fn delete_database(&self, address: &Address) -> Result<(), anyhow::Error> {
        let Some(database) = self.control_db.get_database_by_address(address).await? else {
            return Ok(());
        };
        self.control_db.delete_database(database.id).await?;
        self.schedule_database(None, Some(database)).await?;
        Ok(())
    }

    fn object_db(&self) -> &ObjectDb {
        &self.object_db
    }

    fn control_db(&self) -> &ControlDb {
        &self.control_db
    }

    /// Standalone SpacetimeDB does not support SendGrid as a means to
    /// reissue authentication tokens.
    fn sendgrid_controller(&self) -> Option<&SendGridController> {
        None
    }
}

#[async_trait::async_trait]
impl spacetimedb_client_api::ControlNodeDelegate for StandaloneEnv {
    async fn spacetime_dns(&self, domain: &DomainName) -> spacetimedb::control_db::Result<Option<Address>> {
        self.control_db.spacetime_dns(domain).await
    }

    async fn alloc_spacetime_identity(&self) -> spacetimedb::control_db::Result<Identity> {
        self.control_db.alloc_spacetime_identity().await
    }

    async fn withdraw_energy(&self, identity: &Identity, amount: EnergyQuanta) -> spacetimedb::control_db::Result<()> {
        let energy_balance = self.control_db.get_energy_balance(identity)?;
        let energy_balance = energy_balance.unwrap_or(EnergyQuanta(0));
        log::trace!("Withdrawing {} energy from {}", amount.0, identity);
        log::trace!("Old balance: {}", energy_balance.0);
        let new_balance = energy_balance - amount;
        self.control_db
            .set_energy_balance(*identity, new_balance.as_quanta())
            .await
    }

    fn public_key(&self) -> &DecodingKey {
        &self.public_key
    }
    fn private_key(&self) -> &EncodingKey {
        &self.private_key
    }
}

impl StandaloneEnv {
    async fn insert_database_instance(&self, database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
        let mut new_database_instance = database_instance.clone();
        let id = self.control_db.insert_database_instance(database_instance).await?;
        new_database_instance.id = id;

        self.on_insert_database_instance(&new_database_instance).await?;

        Ok(())
    }

    // Nb. returns `None` if the database was not initialized yet and thus the
    // call was equivalent to create. May change to `Either` once create has a
    // more meaningful result.
    async fn update_database_instance(
        &self,
        database_instance: DatabaseInstance,
    ) -> Result<Option<UpdateDatabaseResult>, anyhow::Error> {
        self.control_db
            .update_database_instance(database_instance.clone())
            .await?;

        self.on_update_database_instance(&database_instance).await
    }

    async fn delete_database_instance(&self, database_instance_id: u64) -> Result<(), anyhow::Error> {
        self.control_db.delete_database_instance(database_instance_id).await?;

        self.on_delete_database_instance(database_instance_id).await;

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

    // TODO(kim): update should only run on the leader instance, and this
    // method should return a single result
    async fn update_database_instances(
        &self,
        database_id: u64,
    ) -> Result<Vec<Option<UpdateDatabaseResult>>, anyhow::Error> {
        let instances = self.control_db.get_database_instances_by_database(database_id).await?;
        let mut results = Vec::with_capacity(instances.len());
        for instance in instances {
            let res = self.update_database_instance(instance).await?;
            results.push(res);
        }

        Ok(results)
    }

    async fn deschedule_replicas(&self, database_id: u64, num_replicas: u32) -> Result<(), anyhow::Error> {
        for _ in 0..num_replicas {
            let instances = self.control_db.get_database_instances_by_database(database_id).await?;
            let Some(instance) = instances.last() else {
                return Ok(());
            };
            self.delete_database_instance(instance.id).await?;
        }
        Ok(())
    }

    async fn on_insert_database_instance(&self, instance: &DatabaseInstance) -> Result<(), anyhow::Error> {
        let state = self.worker_db.get_database_instance_state(instance.id).unwrap();
        if let Some(mut state) = state {
            if !state.initialized {
                // Start and init the service
                self.init_module_on_database_instance(instance.database_id, instance.id)
                    .await?;
                state.initialized = true;
                self.worker_db.upsert_database_instance_state(state).unwrap();
            } else {
                self.start_module_on_database_instance(instance.database_id, instance.id)
                    .await?;
            }
            Ok(())
        } else {
            // Start and init the service
            let mut state = DatabaseInstanceState {
                database_instance_id: instance.id,
                initialized: false,
            };
            self.init_module_on_database_instance(instance.database_id, instance.id)
                .await?;
            self.worker_db.upsert_database_instance_state(state.clone()).unwrap();
            state.initialized = true;
            self.worker_db.upsert_database_instance_state(state).unwrap();
            Ok(())
        }
    }

    async fn on_update_database_instance(
        &self,
        instance: &DatabaseInstance,
    ) -> Result<Option<UpdateDatabaseResult>, anyhow::Error> {
        let state = self.worker_db.get_database_instance_state(instance.id)?;
        match state {
            Some(state) if state.initialized => self
                .update_module_on_database_instance(instance.database_id, instance.id)
                .await
                .map(Some),

            _ => self.on_insert_database_instance(instance).await.map(|()| None),
        }
    }

    async fn on_delete_database_instance(&self, instance_id: u64) {
        let state = self.worker_db.get_database_instance_state(instance_id).unwrap();
        if let Some(_state) = state {
            // TODO(cloutiertyler): We should think about how to clean up
            // database instances which have been deleted. This will just drop
            // them from memory, but will not remove them from disk.  We need
            // some kind of database lifecycle manager long term.
            let (_, scheduler) = self.db_inst_ctx_controller.remove(instance_id).unzip();
            self.host_controller.delete_module_host(instance_id).await.unwrap();
            if let Some(scheduler) = scheduler {
                scheduler.clear();
            }
        }
    }

    async fn load_module_host_context(
        &self,
        database_id: u64,
        instance_id: u64,
    ) -> Result<ModuleHostContext, anyhow::Error> {
        let database = if let Some(database) = self.control_db.get_database_by_id(database_id).await? {
            database
        } else {
            return Err(anyhow::anyhow!(
                "Unknown database/instance: {}/{}",
                database_id,
                instance_id
            ));
        };
        self.load_module_host_context_inner(database, instance_id).await
    }

    async fn load_module_host_context_inner(
        &self,
        database: Database,
        instance_id: u64,
    ) -> anyhow::Result<ModuleHostContext> {
        let program_bytes = self.object_db.get_object(&database.program_bytes_address)?.unwrap();

        let root_db_path = stdb_path("worker_node/database_instances");

        let (dbic, (scheduler, scheduler_starter)) =
            if let Some((dbic, scheduler)) = self.db_inst_ctx_controller.get(instance_id) {
                (dbic, scheduler.new_with_same_db())
            } else {
                let dbic =
                    DatabaseInstanceContext::from_database(self.storage, &database, instance_id, root_db_path.clone());
                let (scheduler, scheduler_starter) = Scheduler::open(dbic.scheduler_db_path(root_db_path))?;
                self.db_inst_ctx_controller.insert(dbic.clone(), scheduler.clone());
                (dbic, (scheduler, scheduler_starter))
            };

        let mhc = ModuleHostContext {
            dbic,
            host_type: database.host_type,
            program_bytes: program_bytes.into(),
            scheduler,
            scheduler_starter,
        };

        Ok(mhc)
    }

    async fn init_module_on_database_instance(&self, database_id: u64, instance_id: u64) -> Result<(), anyhow::Error> {
        let module_host_context = self.load_module_host_context(database_id, instance_id).await?;
        let _address = self.host_controller.init_module_host(module_host_context).await?;
        Ok(())
    }

    async fn start_module_on_database_instance(&self, database_id: u64, instance_id: u64) -> Result<(), anyhow::Error> {
        let module_host_context = self.load_module_host_context(database_id, instance_id).await?;
        let _address = self.host_controller.add_module_host(module_host_context).await?;
        Ok(())
    }

    async fn update_module_on_database_instance(
        &self,
        database_id: u64,
        instance_id: u64,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        let module_host_context = self.load_module_host_context(database_id, instance_id).await?;
        let UpdateOutcome {
            module_host: _,
            update_result,
        } = self.host_controller.update_module_host(module_host_context).await?;

        Ok(update_result)
    }
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
