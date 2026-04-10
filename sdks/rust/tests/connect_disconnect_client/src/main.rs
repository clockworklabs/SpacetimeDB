use connect_disconnect_client::test_handlers;

fn main() {
    let db_name = std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env");
    // Keep the CLI entrypoint thin so both native and wasm execute the same handlers.
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(test_handlers::dispatch(&db_name));
}
