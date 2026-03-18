#[allow(clippy::too_many_arguments)]
#[allow(clippy::large_enum_variant)]
mod module_bindings;
mod pk_test_table;
mod simple_test_table;
mod test_handlers;
mod unique_test_table;

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
