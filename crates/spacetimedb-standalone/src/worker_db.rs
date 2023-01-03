use prost::Message;
use spacetimedb::protobuf::worker_db::DatabaseInstanceState;

// TODO: Consider making not static
lazy_static::lazy_static! {
    static ref WORKER_DB: sled::Db = init().unwrap();
}

fn init() -> Result<sled::Db, anyhow::Error> {
    let config = sled::Config::default()
        .path("/stdb/worker_node/worker_db")
        .flush_every_ms(Some(50))
        .mode(sled::Mode::HighThroughput);
    let db = config.open()?;
    Ok(db)
}

pub fn upsert_database_instance_state(state: DatabaseInstanceState) -> Result<(), anyhow::Error> {
    let tree = WORKER_DB.open_tree("worker_database_instance")?;

    let mut buf = Vec::new();
    state.encode(&mut buf).unwrap();

    tree.insert(state.database_instance_id.to_be_bytes(), buf.clone())?;
    Ok(())
}

pub fn get_database_instance_state(database_instance_id: u64) -> Result<Option<DatabaseInstanceState>, anyhow::Error> {
    let tree = WORKER_DB.open_tree("worker_database_instance")?;

    if let Some(value) = tree.get(database_instance_id.to_be_bytes())? {
        let state = DatabaseInstanceState::decode(&value[..])?;
        Ok(Some(state))
    } else {
        Ok(None)
    }
}
