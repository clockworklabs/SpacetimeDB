use log::*;
use spacetimedb::routes::router;
use spacetimedb::wasm_host::Host;
use tokio::runtime::Builder;
use tokio::{fs, spawn};
use std::error::Error;
use std::net::SocketAddr;
use wasmer::wat2wasm;

async fn async_main() -> Result<(), Box<dyn Error + Send + Sync>> {
    configure_logging();

    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/rust-wasm-test/wat")).await.unwrap();
    let wat = fs::read(path).await?;
    // println!("{}", String::from_utf8(wat.to_owned()).unwrap());
    let wasm_bytes = wat2wasm(&wat)?.to_vec();
    let host = Host::new();
    let reducer = host.add_reducer(wasm_bytes).await?;
    host.run_reducer(reducer).await?;
    //host.run_reducer(reducer).await?;S

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
