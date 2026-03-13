#![allow(clippy::disallowed_macros)]

#[path = "main.rs"]
mod cli;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(_test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    // The shared wasm test harness always passes `(test_name, db_name)`, even for
    // fixed-flow clients like this one that ignore the selector.
    cli::set_web_db_name(db_name);
    cli::dispatch().await;
}
