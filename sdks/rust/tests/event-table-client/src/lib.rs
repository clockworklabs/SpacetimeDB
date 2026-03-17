#![allow(clippy::disallowed_macros)]

// The shared client implementation lives in `main.rs`, but only the wasm build
// needs to compile it as a library module. Native uses the binary entrypoint.
#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[path = "main.rs"]
mod cli;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    // The shared wasm test harness passes both the selected test name and the
    // published database name. wasm clients cannot rely on the native env-var path.
    cli::set_web_db_name(db_name);
    cli::dispatch(&test_name).await;
}
