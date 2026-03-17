#![allow(clippy::disallowed_macros)]

// This crate has two entrypoints:
// - the native CLI binary in `main.rs`
// - the wasm `run(...)` export below
//
// Only the wasm build needs to include `main.rs` as a library module. Pulling the
// same file into the native library target makes clippy analyze a second, unused
// copy of the entire test client implementation and emit dead-code noise.
#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[path = "main.rs"]
mod cli;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub(crate) use cli::module_bindings;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    cli::set_web_db_name(db_name);
    cli::dispatch(&test_name).await;
}
