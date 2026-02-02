#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
mod wasm_tests {
    mod module_bindings;

    use module_bindings::{DbConnection, RemoteDbContext};
    use wasm_bindgen_test::wasm_bindgen_test;

    const LOCALHOST: &str = "http://localhost:3000";

    fn db_name_or_panic() -> String {
        std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env")
    }

    #[wasm_bindgen_test]
    fn wasm_smoke_connect_and_disconnect() {
        let name = db_name_or_panic();

        let conn = DbConnection::builder()
            .with_module_name(name)
            .with_uri(LOCALHOST)
            .build()
            .expect("Failed to build DbConnection");

        // Basic smoke: immediately disconnect. If the wasm build is broken (missing web feature
        // plumbing), this tends to fail before here.
        conn.disconnect().expect("disconnect failed");
    }
}
