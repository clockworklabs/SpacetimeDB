use spacetimedb::messages::worker_db::DatabaseInstanceState;
use spacetimedb_lib::bsatn;

pub struct WorkerDb {
    db: sled::Db,
}

impl WorkerDb {
    pub fn init() -> Result<Self, anyhow::Error> {
        let config = sled::Config::default()
            .path(spacetimedb::stdb_path("worker_node/worker_db"))
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput);
        let db = config.open()?;
        Ok(Self { db })
    }
}

impl WorkerDb {
    pub fn upsert_database_instance_state(&self, state: DatabaseInstanceState) -> Result<(), anyhow::Error> {
        let tree = self.db.open_tree("worker_database_instance")?;

        let buf = bsatn::to_vec(&state).unwrap();

        tree.insert(state.database_instance_id.to_be_bytes(), buf)?;
        Ok(())
    }

    pub fn get_database_instance_state(
        &self,
        database_instance_id: u64,
    ) -> Result<Option<DatabaseInstanceState>, anyhow::Error> {
        let tree = self.db.open_tree("worker_database_instance")?;

        if let Some(value) = tree.get(database_instance_id.to_be_bytes())? {
            let state = bsatn::from_slice(&value[..])?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }
}
