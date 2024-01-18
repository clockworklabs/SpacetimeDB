macro_rules! declare_tests_with_suffix {
    ($lang:ident, $suffix:literal) => {
        mod $lang {
            use spacetimedb_testing::sdk::Test;

            const MODULE: &str = concat!("sdk-test", $suffix);
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

            #[test]
            fn insert_identity() {
                make_test("insert_identity").run();
            }

            #[test]
            fn delete_identity() {
                make_test("delete_identity").run();
            }

            #[test]
            fn update_identity() {
                make_test("delete_identity").run();
            }

            #[test]
            fn insert_address() {
                make_test("insert_address").run();
            }

            #[test]
            fn delete_address() {
                make_test("delete_address").run();
            }

            #[test]
            fn update_address() {
                make_test("delete_address").run();
            }

            #[test]
            fn on_reducer() {
                make_test("on_reducer").run();
            }

            #[test]
            fn fail_reducer() {
                make_test("fail_reducer").run();
            }

            #[test]
            fn insert_vec() {
                make_test("insert_vec").run();
            }

            #[test]
            fn insert_simple_enum() {
                make_test("insert_simple_enum").run();
            }

            #[test]
            fn insert_enum_with_payload() {
                make_test("insert_enum_with_payload").run();
            }

            #[test]
            fn insert_long_table() {
                make_test("insert_long_table").run();
            }

            #[test]
            fn resubscribe() {
                make_test("resubscribe").run();
            }

            #[test]
            #[should_panic]
            fn should_fail() {
                make_test("should_fail").run();
            }

            #[test]
            fn reauth() {
                make_test("reauth_part_1").run();
                make_test("reauth_part_2").run();
            }

            #[test]
            fn reconnect_same_address() {
                make_test("reconnect_same_address").run();
            }

            #[test]
            fn connect_disconnect_callbacks() {
                Test::builder()
                    .with_name(concat!("connect_disconnect_callback_", stringify!($lang)))
                    .with_module(concat!("sdk-test-connect-disconnect", $suffix))
                    .with_client(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/connect_disconnect_client"
                    ))
                    .with_language("rust")
                    .with_bindings_dir("src/module_bindings")
                    .with_compile_command("cargo build")
                    .with_run_command("cargo run")
                    .build()
                    .run();
            }
        }
    };
}

declare_tests_with_suffix!(rust, "");
declare_tests_with_suffix!(csharp, "-cs");
