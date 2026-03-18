#![allow(clippy::disallowed_macros)]

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod pk_test_table;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod simple_test_table;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod test_handlers;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod unique_test_table;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen]
pub async fn run(test_name: String, db_name: String) {
    console_error_panic_hook::set_once();
    test_handlers::dispatch(&test_name, &db_name).await;
}
