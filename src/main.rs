use spacetimedb::wasm_host::Host;
use tokio::runtime::Builder;
use tokio::fs;
use std::error::Error;
use wasmer::wat2wasm;

async fn async_main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/rust-wasm-test/wat")).await.unwrap();
    let wat = fs::read(path).await?;
    // println!("{}", String::from_utf8(wat.to_owned()).unwrap());
    let wasm_bytes = wat2wasm(&wat)?.to_vec();
    let host = Host::new();
    let reducer = host.add_reducer(wasm_bytes).await?;
    host.run_reducer(reducer).await?;
    //host.run_reducer(reducer).await?;
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

