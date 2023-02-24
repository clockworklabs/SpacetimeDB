use spacetimedb::address::Address;
use spacetimedb::control_db::ControlDb;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::host::host_controller;
use spacetimedb::identity::Identity;
use spacetimedb::object_db::ObjectDb;
use spacetimedb::protobuf::control_db::HostType;
use spacetimedb::protobuf::worker_db::DatabaseInstanceState;
use spacetimedb::worker_database_instance::WorkerDatabaseInstance;
use spacetimedb::{
    hash::Hash,
    protobuf::control_db::{Database, DatabaseInstance},
};

use crate::worker_db::WorkerDb;

pub struct Controller {
    worker_db: WorkerDb,
    control_db: &'static ControlDb,
    pub(super) db_inst_ctx_controller: DatabaseInstanceContextController,
    object_db: ObjectDb,
}

impl Controller {
    pub fn new(
        worker_db: WorkerDb,
        control_db: &'static ControlDb,
        db_inst_ctx_controller: DatabaseInstanceContextController,
        object_db: ObjectDb,
    ) -> Self {
        Self {
            worker_db,
            control_db,
            db_inst_ctx_controller,
            object_db,
        }
    }
}

spacetimedb_client_api::delegate_databasedb!(for Controller, self to self.control_db, |x| x.await);

#[async_trait::async_trait]
impl spacetimedb_client_api::Controller for Controller {
    async fn insert_database(
        &self,
        address: &Address,
        identity: &Identity,
        program_bytes_address: &Hash,
        host_type: HostType,
        num_replicas: u32,
        force: bool,
        trace_log: bool,
    ) -> Result<(), anyhow::Error> {
        let database = Database {
            id: 0,
            address: address.as_slice().to_vec(),
            identity: identity.as_slice().to_owned(),
            host_type: host_type as i32,
            num_replicas,
            program_bytes_address: program_bytes_address.as_slice().to_owned(),
            trace_log,
        };

        if force {
            if let Some(database) = self.control_db.get_database_by_address(address).await? {
                let database_id = database.id;
                self.schedule_database(None, Some(database)).await?;
                self.control_db.delete_database(database_id).await?;
            }
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
    ) -> Result<(), anyhow::Error> {
        let database = self.control_db.get_database_by_address(address).await?;
        let mut database = match database {
            Some(database) => database,
            None => return Ok(()),
        };

        let old_database = database.clone();

        database.program_bytes_address = program_bytes_address.as_slice().to_vec();
        database.num_replicas = num_replicas;
        let new_database = database.clone();
        self.control_db.update_database(database).await?;

        self.schedule_database(Some(new_database), Some(old_database)).await?;

        Ok(())
    }

    async fn delete_database(&self, address: &Address) -> Result<(), anyhow::Error> {
        let database = self.control_db.get_database_by_address(address).await?;
        let database = match database {
            Some(database) => database,
            None => return Ok(()),
        };
        self.control_db.delete_database(database.id).await?;

        self.schedule_database(None, Some(database)).await?;

        Ok(())
    }

    fn object_db(&self) -> &ObjectDb {
        &self.object_db
    }
}

impl Controller {
    async fn insert_database_instance(&self, database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
        let mut new_database_instance = database_instance.clone();
        let id = self.control_db.insert_database_instance(database_instance).await?;
        new_database_instance.id = id;

        self.on_insert_database_instance(&new_database_instance).await?;

        Ok(())
    }

    async fn _update_database_instance(&self, database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
        self.control_db
            ._update_database_instance(database_instance.clone())
            .await?;

        self._on_update_database_instance(&database_instance).await?;

        Ok(())
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

    async fn _on_update_database_instance(&self, instance: &DatabaseInstance) -> Result<(), anyhow::Error> {
        // This logic is the same right now
        self.on_insert_database_instance(instance).await
    }

    async fn on_delete_database_instance(&self, instance_id: u64) {
        let state = self.worker_db.get_database_instance_state(instance_id).unwrap();
        if let Some(_state) = state {
            let host = host_controller::get_host();

            // TODO: This is getting pretty messy
            self.db_inst_ctx_controller.remove(instance_id);
            host.delete_module(instance_id).await.unwrap();
        }
    }

    async fn init_module_on_database_instance(&self, database_id: u64, instance_id: u64) -> Result<(), anyhow::Error> {
        let database = if let Some(database) = self.control_db.get_database_by_id(database_id).await? {
            database
        } else {
            return Err(anyhow::anyhow!(
                "Unknown database/instance: {}/{}",
                database_id,
                instance_id
            ));
        };
        let identity = Hash::from_slice(&database.identity);
        let address = Address::from_slice(database.address);
        let program_bytes_address = Hash::from_slice(&database.program_bytes_address);
        let program_bytes = self.object_db.get_object(&program_bytes_address)?.unwrap();

        let log_path = DatabaseLogger::filepath(&address, instance_id);
        let root = "/stdb/worker_node/database_instances";
        let db_path = format!("{}/{}/{}/{}", root, address.to_hex(), instance_id, "database");

        let worker_database_instance = WorkerDatabaseInstance::new(
            instance_id,
            database_id,
            HostType::from_i32(database.host_type).expect("unknown module host type"),
            database.trace_log,
            identity,
            address,
            &db_path,
            &log_path,
        );

        // TODO: This is getting pretty messy
        self.db_inst_ctx_controller.insert(worker_database_instance.clone());
        let host = host_controller::get_host();
        let _address = host
            .init_module(worker_database_instance, program_bytes.clone())
            .await?;
        Ok(())
    }

    async fn start_module_on_database_instance(&self, database_id: u64, instance_id: u64) -> Result<(), anyhow::Error> {
        let database = if let Some(database) = self.control_db.get_database_by_id(database_id).await? {
            database
        } else {
            return Err(anyhow::anyhow!(
                "Unknown database/instance: {}/{}",
                database_id,
                instance_id
            ));
        };
        let host_type = database.host_type();
        let identity = Hash::from_slice(&database.identity);
        let address = Address::from_slice(database.address);
        let program_bytes_address = Hash::from_slice(&database.program_bytes_address);
        let program_bytes = self.object_db.get_object(&program_bytes_address)?.unwrap();

        let log_path = DatabaseLogger::filepath(&address, instance_id);
        let root = "/stdb/worker_node/database_instances";
        let db_path = format!("{}/{}/{}/{}", root, address.to_hex(), instance_id, "database");

        let worker_database_instance = WorkerDatabaseInstance::new(
            instance_id,
            database_id,
            host_type,
            database.trace_log,
            identity,
            address,
            db_path,
            log_path,
        );

        // TODO: This is getting pretty messy
        self.db_inst_ctx_controller.insert(worker_database_instance.clone());
        let host = host_controller::get_host();
        let _address = host.add_module(worker_database_instance, program_bytes.clone()).await?;
        Ok(())
    }
}
