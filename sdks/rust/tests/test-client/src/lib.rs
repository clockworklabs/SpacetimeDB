#![allow(clippy::disallowed_macros)]

#[path = "main.rs"]
mod cli;

pub(crate) use cli::module_bindings;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    cli::set_web_db_name(db_name);
    cli::dispatch_async(&test_name).await;
}
