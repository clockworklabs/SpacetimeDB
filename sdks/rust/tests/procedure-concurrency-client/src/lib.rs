#![allow(clippy::disallowed_macros)]

mod module_bindings;
pub mod test_handlers;

#[cfg(all(target_arch = "wasm32", feature = "browser"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "browser"))]
#[wasm_bindgen]
pub async fn run(test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    test_handlers::dispatch(&test_name, &db_name).await;
}
