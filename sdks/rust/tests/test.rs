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
                make_test("insert-primitive").run();
            }

            #[test]
            fn subscribe_and_cancel() {
                make_test("subscribe-and-cancel").run();
            }

            #[test]
            fn subscribe_and_unsubscribe() {
                make_test("subscribe-and-unsubscribe").run();
            }

            #[test]
            fn subscription_error_smoke_test() {
                make_test("subscription-error-smoke-test").run();
            }
            #[test]
            fn delete_primitive() {
                make_test("delete-primitive").run();
            }

            #[test]
            fn update_primitive() {
                make_test("update-primitive").run();
            }

            #[test]
            fn insert_identity() {
                make_test("insert-identity").run();
            }

            #[test]
            fn insert_caller_identity() {
                make_test("insert-caller-identity").run();
            }

            #[test]
            fn delete_identity() {
                make_test("delete-identity").run();
            }

            #[test]
            fn update_identity() {
                make_test("delete-identity").run();
            }

            #[test]
            fn insert_connection_id() {
                make_test("insert-connection-id").run();
            }

            #[test]
            fn insert_caller_connection_id() {
                make_test("insert-caller-connection-id").run();
            }

            #[test]
            fn delete_connection_id() {
                make_test("delete-connection-id").run();
            }

            #[test]
            fn update_connection_id() {
                make_test("delete-connection-id").run();
            }

            #[test]
            fn insert_timestamp() {
                make_test("insert-timestamp").run();
            }

            #[test]
            fn insert_call_timestamp() {
                make_test("insert-call-timestamp").run();
            }

            #[test]
            fn on_reducer() {
                make_test("on-reducer").run();
            }

            #[test]
            fn fail_reducer() {
                make_test("fail-reducer").run();
            }

            #[test]
            fn insert_vec() {
                make_test("insert-vec").run();
            }

            #[test]
            fn insert_option_some() {
                make_test("insert-option-some").run();
            }

            #[test]
            fn insert_option_none() {
                make_test("insert-option-none").run();
            }

            #[test]
            fn insert_struct() {
                make_test("insert-struct").run();
            }

            #[test]
            fn insert_simple_enum() {
                make_test("insert-simple-enum").run();
            }

            #[test]
            fn insert_enum_with_payload() {
                make_test("insert-enum-with-payload").run();
            }

            #[test]
            fn insert_delete_large_table() {
                make_test("insert-delete-large-table").run();
            }

            #[test]
            fn insert_primitives_as_strings() {
                make_test("insert-primitives-as-strings").run();
            }

            // #[test]
            // fn resubscribe() {
            //     make_test("resubscribe").run();
            // }

            #[test]
            #[should_panic]
            fn should_fail() {
                make_test("should-fail").run();
            }

            #[test]
            fn reauth() {
                make_test("reauth-part-1").run();
                make_test("reauth-part-2").run();
            }

            #[test]
            fn reconnect_different_connection_id() {
                make_test("reconnect-different-connection-id").run();
            }

            #[test]
            fn connect_disconnect_callbacks() {
                Test::builder()
                    .with_name(concat!("connect-disconnect-callback-", stringify!($lang)))
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

            #[test]
            fn caller_always_notified() {
                make_test("caller-always-notified").run();
            }

            #[test]
            fn subscribe_all_select_star() {
                make_test("subscribe-all-select-star").run();
            }

            #[test]
            fn caller_alice_receives_reducer_callback_but_not_bob() {
                make_test("caller-alice-receives-reducer-callback-but-not-bob").run();
            }

            #[test]
            fn row_deduplication() {
                make_test("row-deduplication").run();
            }

            #[test]
            fn row_deduplication_join_r_and_s() {
                make_test("row-deduplication-join-r-and-s").run();
            }

            #[test]
            fn row_deduplication_r_join_s_and_r_join_t8() {
                make_test("row-deduplication-r-join-s-and-r-joint").run();
            }

            #[test]
            fn test_lhs_join_update() {
                make_test("test-lhs-join-update").run()
            }

            #[test]
            fn test_lhs_join_update_disjoint_queries() {
                make_test("test-lhs-join-update-disjoint-queries").run()
            }

            #[test]
            fn test_intra_query_bag_semantics_for_join() {
                make_test("test-intra-query-bag-semantics-for-join").run()
            }

            #[test]
            fn two_different_compression_algos() {
                make_test("two-different-compression-algos").run();
            }

            #[test]
            fn test_parameterized_subscription() {
                make_test("test-parameterized-subscription").run();
            }

            #[test]
            fn test_rls_subscription() {
                make_test("test-rls-subscription").run()
            }

            #[test]
            fn pk_simple_enum() {
                make_test("pk-simple-enum").run();
            }

            #[test]
            fn indexed_simple_enum() {
                make_test("indexed-simple-enum").run();
            }

            #[test]
            fn overlapping_subscriptions() {
                make_test("overlapping-subscriptions").run();
            }
        }
    };
}

declare_tests_with_suffix!(rust, "");
declare_tests_with_suffix!(typescript, "-ts");
// TODO: migrate csharp to snake_case table names
declare_tests_with_suffix!(csharp, "-cs");
