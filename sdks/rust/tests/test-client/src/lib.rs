<<<<<<< Updated upstream
#![allow(clippy::disallowed_macros)]

#[path = "main.rs"]
mod cli;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(test_name: String) {
    cli::dispatch(&test_name);
}
||||||| Stash base
=======
#![allow(clippy::disallowed_macros)]

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
const LOCALHOST: &str = "http://localhost:3000";

#[cfg(all(target_arch = "wasm32", feature = "web"))]
fn db_name_or_panic() -> String {
    std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(test_name: String) {
    match test_name.as_str() {
        "wasm-smoke-connect" => wasm_smoke_connect().await,
        _ => panic!("Unknown test: {test_name}"),
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
async fn wasm_smoke_connect() {
    mod module_bindings;
    use module_bindings::DbConnection;

    let name = db_name_or_panic();
    let conn = DbConnection::builder()
        .with_module_name(name)
        .with_uri(LOCALHOST)
        .build()
        .await
        .expect("Failed to build DbConnection");

    conn.disconnect().expect("disconnect failed");
}
>>>>>>> Stashed changes
