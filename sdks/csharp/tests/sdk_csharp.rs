use serial_test::serial;
use spacetimedb_testing::sdk::Test;

const TEST_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sdk-test-client");
const CONNECT_DISCONNECT_CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/connect-disconnect-client");

fn make_test(subcommand: &str) -> Test {
    Test::builder()
        .with_name(format!("csharp-client-{subcommand}"))
        .with_module("sdk-test-cs")
        .with_client(TEST_CLIENT)
        .with_language("csharp")
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command(format!("dotnet ./bin~/Debug/net8.0/sdk-test-client.dll {subcommand}"))
        .build()
}

#[test]
#[serial(Group1)]
fn insert_primitive() {
    make_test("insert-primitive").run();
}

#[test]
#[serial(Group1)]
fn delete_primitive() {
    make_test("delete-primitive").run();
}

#[test]
#[serial(Group1)]
fn update_primitive() {
    make_test("update-primitive").run();
}

#[test]
#[serial(Group1)]
fn insert_builtin() {
    make_test("insert-builtin").run();
}

#[test]
#[serial(Group2)]
fn insert_vec() {
    make_test("insert-vec").run();
}

#[test]
#[serial(Group2)]
fn insert_option_some() {
    make_test("insert-option-some").run();
}

#[test]
#[serial(Group2)]
fn insert_option_none() {
    make_test("insert-option-none").run();
}

#[test]
#[serial(Group2)]
fn insert_struct() {
    make_test("insert-struct").run();
}

#[test]
#[serial(Group3)]
fn insert_simple_enum() {
    make_test("insert-simple-enum").run();
}

#[test]
#[serial(Group3)]
fn insert_enum_with_payload() {
    make_test("insert-enum-with-payload").run();
}

#[test]
#[serial(Group3)]
fn fail_reducer() {
    make_test("fail-reducer").run();
}

#[test]
#[serial(Group3)]
fn caller_always_notified() {
    make_test("caller-always-notified").run();
}

#[test]
#[serial(Group4)]
fn connect_disconnect_callbacks() {
    Test::builder()
        .with_name("csharp-client-connect-disconnect-callbacks")
        .with_module("sdk-test-connect-disconnect-cs")
        .with_client(CONNECT_DISCONNECT_CLIENT)
        .with_language("csharp")
        .with_bindings_dir("module_bindings")
        .with_compile_command("bash ../build-client.sh")
        .with_run_command("dotnet ./bin~/Debug/net8.0/connect-disconnect-client.dll")
        .build()
        .run();
}
