use log::*;
use spacetimedb::api::{self, MODULE_ODB};
use spacetimedb::hash::{hash_bytes, Hash};
use spacetimedb::postgres;
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
        wasm_host::get_host()
            .add_module(identity, name, wasm_bytes)
            .await
            .unwrap();
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
    let hex_identity = hex::encode(hash_bytes(""));
    let name = "test";
    if let Err(e) = api::database::init_module(&hex_identity, name, wasm_bytes).await {
        // TODO: check if it failed because it's already been created
        log::error!("{:?}", e);
    }

    let reducer: String = "test".into();

    // TODO: actually handle args
    let arg_str = r#"[{"x": 0, "y": 1, "z": 2}, {"foo": "This is a string."}]"#;
    let arg_bytes = arg_str.as_bytes().to_vec();
    api::database::call(&hex_identity, &name, reducer.clone(), arg_bytes.clone()).await?;
    api::database::call(&hex_identity, &name, reducer, arg_bytes).await?;

    println!("logs:");
    println!("{}", api::database::logs(&hex_identity, &name, 10).await);

    let (identity, token) = api::spacetime_identity().await?;
    println!("identity: {:?}", identity);
    println!("token: {}", token);

    api::spacetime_identity_associate_email("tyler@clockworklabs.io", &token).await?;
    //////////////////

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
