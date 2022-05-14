use log::*;
use spacetimedb::hash::Hash;
use spacetimedb::db::persistent_object_db::odb;
use spacetimedb::postgres;
use spacetimedb::api;
use spacetimedb::routes::router;
use spacetimedb::wasm_host;
use std::error::Error;
use std::net::SocketAddr;
use tokio::runtime::Builder;
use tokio::{fs, spawn};
use wasmer::wat2wasm;
// use gluesql::prelude::*;

async fn startup() {
    // TODO: maybe replace storage layer with something like rocksdb or sled
    // let storage = SledStorage::new("data/doc-db").unwrap();
    // let mut glue = Glue::new(storage);
    // let x = glue.execute("");
    // if let Ok(x) = x {
    //     let y: i32 = x.into();
    // }

    let client = postgres::get_client().await;
    let result = client.query(r"
        SELECT DISTINCT ON (actor_name, st_identity, module_version)
            actor_name, st_identity, module_version, module_address
        FROM registry.module
        ORDER BY module_version DESC;", &[]).await;
    let rows = result.unwrap();
    for row in rows {
        let module_address: String = row.get(3);
        let hash: Hash = Hash::from_iter(hex::decode(module_address).unwrap());
        let wasm_bytes = odb::get(hash).await.unwrap();
        wasm_host::get_host().init_module(wasm_bytes.clone()).await.unwrap();
    }
}

async fn async_main() -> Result<(), Box<dyn Error + Send + Sync>> {
    configure_logging();

    postgres::init().await;

    startup().await;

    //////////////////
    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"), "/../rust-wasm-test/wat"))
        .await
        .unwrap();
    let wat = fs::read(path).await?;

    // println!("{}", String::from_utf8(wat.to_owned()).unwrap());

    let wasm_bytes = wat2wasm(&wat)?.to_vec();
    let identity: String = "clockworklabs".into();
    let name: String = "bitcraft".into();
    if let Err(_) = api::database::init_module(identity.clone(), name.clone(), wasm_bytes).await {
        // TODO: check if it failed because it's already been created
    }

    let reducer = "test".into();

    let arg_data = vec![0, 0, 0];
    api::database::call(identity, name, reducer, arg_data).await?;
    //////////////////

    spawn(async move {
        // Start https server
        let addr = SocketAddr::from(([0, 0, 0, 0], 3010));

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
