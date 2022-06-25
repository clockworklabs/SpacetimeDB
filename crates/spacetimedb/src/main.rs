use log::*;
use spacetimedb::api::MODULE_ODB;
use spacetimedb::clients::client_connection_index::ClientActorIndex;
use spacetimedb::hash::Hash;
use spacetimedb::postgres;
use spacetimedb::routes::router;
use spacetimedb::wasm_host;
use spacetimedb::metrics;
use std::error::Error;
use std::net::SocketAddr;
use tokio::runtime::Builder;
use tokio::spawn;

async fn startup() {
    // TODO: maybe replace storage layer with something like rocksdb or sled
    // let storage = SledStorage::new("data/doc-db").unwrap();
    // let mut glue = Glue::new(storage);
    // let x = glue.execute("");
    // if let Ok(x) = x {
    //     let y: i32 = x.into();
    // }
    ClientActorIndex::start_liveliness_check();

    let client = postgres::get_client().await;
    let result = client
        .query(
            r"
        SELECT DISTINCT ON (actor_name, st_identity, module_version)
            actor_name, st_identity, module_version, module_address
        FROM registry.module
        ORDER BY module_version DESC;",
            &[],
        )
        .await;
    let rows = result.unwrap();

    for row in rows {
        let name: String = row.get(0);
        let hex_identity: String = row.get(1);
        let identity = *Hash::from_slice(&hex::decode(hex_identity).unwrap());

        // let version: i32 = row.get(2);
        // let address: String = row.get(3);
        let module_address: String = row.get(3);
        let hash: Hash = Hash::from_iter(hex::decode(module_address).unwrap());
        let wasm_bytes = {
            let object_db = MODULE_ODB.lock().unwrap();
            object_db.get(hash).unwrap().to_vec()
        };

        let host = wasm_host::get_host();
        host.add_module(identity, name, wasm_bytes).await.unwrap();
    }
}

async fn async_main() -> Result<(), Box<dyn Error + Send + Sync>> {
    configure_logging();
    metrics::register_custom_metrics();
    postgres::init().await;
    startup().await;

    spawn(async move {
        // Start https server
        let addr = SocketAddr::from(([0, 0, 0, 0], 3000));

        debug!("Listening for http requests at http://{}", addr);
        gotham::init_server(addr, router()).await.unwrap();
    })
    .await?;

    Ok(())
}

fn main() {
    // Create a single threaded run loop
    Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
        .unwrap();
}

fn configure_logging() {
    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();
}
