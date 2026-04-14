#![allow(clippy::disallowed_macros)]

#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;
mod pk_test_table;
mod simple_test_table;
pub mod test_handlers;
mod unique_test_table;

#[cfg(all(target_arch = "wasm32", feature = "browser"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "browser"))]
#[wasm_bindgen]
pub async fn run(test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    test_handlers::dispatch(&test_name, &db_name).await;
}
