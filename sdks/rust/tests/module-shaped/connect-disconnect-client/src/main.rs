use module_shaped_connect_disconnect_client::test_handlers;

fn main() {
    let db_name = std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env");
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(test_handlers::dispatch(&db_name));
}
