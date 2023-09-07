use spacetimedb_testing::sdk::Test;

const MODULE: &str = "sdk-test";
const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test-client");

fn make_test(subcommand: &str) -> Test {
    Test::builder()
        .with_name(subcommand)
        .with_module(MODULE)
        .with_client(CLIENT)
        .with_language("rust")
        .with_bindings_dir("src/module_bindings")
        .with_compile_command("cargo build")
        .with_run_command(format!("cargo run -- {}", subcommand))
        .build()
}

#[test]
fn insert_primitive() {
    make_test("insert_primitive").run();
}

#[test]
fn delete_primitive() {
    make_test("delete_primitive").run();
}

#[test]
fn update_primitive() {
    make_test("update_primitive").run();
}
