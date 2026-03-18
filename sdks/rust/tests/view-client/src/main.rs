mod module_bindings;
mod test_handlers;

fn main() {
    env_logger::init();
    test_handlers::exit_on_panic();

    let test = std::env::args()
        .nth(1)
        .expect("Pass a test name as a command-line argument to the test client");
    let db_name = std::env::var("SPACETIME_SDK_TEST_DB_NAME").expect("Failed to read db name from env");

    // Keep the CLI entrypoint thin so both native and wasm execute the same handlers.
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(test_handlers::dispatch(&test, &db_name));
}
