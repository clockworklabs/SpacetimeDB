#[cfg(feature = "browser")]
use std::path::Path;

use spacetimedb_testing::sdk::{Test, TestBuilder};

fn platform_test_builder(client_project: &str, run_selector: Option<&str>) -> TestBuilder {
    let builder = Test::builder();
    let builder = builder.with_client(client_project);

    // Note: `run_selector` is intentionally interpreted differently by mode:
    // - Native mode uses it as a CLI subcommand (`cargo run -- <selector>`), with `None` => `cargo run`.
    // - Web mode assembles the Node/wasm-bindgen commands directly in this test harness.
    #[cfg(feature = "browser")]
    {
        let package_name = Path::new(client_project)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("client project path should end in a UTF-8 directory name")
            .to_owned();
        let artifact_name = package_name.replace('-', "_");
        let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
            // Cargo workspace members emit into the workspace target directory, not each crate's
            // local `./target`. Use `CARGO_TARGET_DIR` when set, otherwise fall back to the
            // workspace target.
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .to_string_lossy()
                .into_owned()
        });
        let bindgen_out_dir = format!("{client_project}/target/sdk-test-web-bindgen/{package_name}");
        let wasm_path = format!("{target_dir}/wasm32-unknown-unknown/debug/deps/{artifact_name}.wasm");
        let js_module = format!("{bindgen_out_dir}/{artifact_name}.js");
        let js_module_cjs = format!("{bindgen_out_dir}/{artifact_name}.cjs");
        let build_command = "cargo build --target wasm32-unknown-unknown --no-default-features --features browser";
        let mkdir_command = shlex::try_join(["mkdir", "-p", bindgen_out_dir.as_str()])
            .expect("bindgen output path should be shell-quotable");
        let bindgen_command = shlex::try_join([
            "wasm-bindgen",
            "--target",
            "nodejs",
            "--out-dir",
            bindgen_out_dir.as_str(),
            wasm_path.as_str(),
        ])
        .expect("wasm-bindgen command should be shell-quotable");
        let cp_command = shlex::try_join(["cp", js_module.as_str(), js_module_cjs.as_str()])
            .expect("bindgen JS output paths should be shell-quotable");
        let compile_command = format!(
            "/bin/bash -lc \
             \"{build_command} \
             && {mkdir_command} \
             && {bindgen_command} \
             && {cp_command}\""
        );
        let js_module = format!("{bindgen_out_dir}/{artifact_name}.cjs");
        let run_selector = run_selector.unwrap_or_default();
        let node_script = format!(
            "(async () => {{ \
              const m = require({js_module:?}); \
              if (m.default) {{ await m.default(); }} \
              const run = m.run || m.main || m.start; \
              if (!run) throw new Error(\"No exported run/main/start function from wasm module\"); \
              const dbName = process.env.SPACETIME_SDK_TEST_DB_NAME; \
              if (!dbName) throw new Error(\"Missing SPACETIME_SDK_TEST_DB_NAME\"); \
              await run({run_selector:?}, dbName); \
              // These wasm clients run under Node rather than a browser. Some tests intentionally leave
              // websocket/event-loop work alive once their assertions are complete, so exit here to keep
              // non-lifecycle tests from hanging on leftover handles after `run()` has finished.
              process.exit(0);
            }})().catch((e) => {{ console.error(e); process.exit(1); }});"
        );
        let node_script = shlex::try_quote(&node_script).expect("inline Node script should be shell-quotable");
        let run_command = format!("node --experimental-websocket -e {node_script}");

        builder
            .with_compile_command(compile_command)
            .with_run_command(run_command)
    }

    #[cfg(not(feature = "browser"))]
    {
        let run_command = match run_selector {
            Some(subcommand) => format!("cargo run -- {}", subcommand),
            None => "cargo run".to_owned(),
        };

        builder
            .with_compile_command("cargo build")
            .with_run_command(run_command)
    }
}

macro_rules! maybe_reducer_return_test {
    (rust, $make_test:ident) => {
        #[test]
        fn reducer_return_values() {
            $make_test("reducer-return-values").run();
        }
    };
    ($other:ident, $make_test:ident) => {};
}

macro_rules! declare_tests_with_suffix {
    ($lang:ident, $suffix:literal) => {
        mod $lang {
            use spacetimedb_testing::sdk::Test;

            const MODULE: &str = concat!("sdk-test", $suffix);
            const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test-client");

            fn make_test(subcommand: &str) -> Test {
                super::platform_test_builder(CLIENT, Some(subcommand))
                    .with_name(subcommand)
                    .with_module(MODULE)
                    .with_language("rust")
                    // We test against multiple modules in different languages,
                    // and as of writing (pgoldman 2026-02-12),
                    // some of those languages have not yet been updated to make scheduled and lifecycle reducers
                    // private by default. As such, generating only public items results in different bindings
                    // depending on which module is the source.
                    .with_generate_private_items(true)
                    .with_bindings_dir("src/module_bindings")
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
            fn insert_call_uuid_v4() {
                make_test("insert-call-uuid-v4").run();
            }

            #[test]
            fn insert_call_uuid_v7() {
                make_test("insert-call-uuid-v7").run();
            }

            #[test]
            fn insert_uuid() {
                make_test("insert-uuid").run();
            }

            #[test]
            fn delete_uuid() {
                make_test("delete-uuid").run();
            }

            #[test]
            fn update_uuid() {
                make_test("delete-uuid").run();
            }

            #[test]
            fn on_reducer() {
                make_test("on-reducer").run();
            }

            #[test]
            fn fail_reducer() {
                make_test("fail-reducer").run();
            }

            maybe_reducer_return_test!($lang, make_test);

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
                const CONNECT_DISCONNECT_CLIENT: &str =
                    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/connect_disconnect_client");

                super::platform_test_builder(CONNECT_DISCONNECT_CLIENT, None)
                    .with_name(concat!("connect-disconnect-callback-", stringify!($lang)))
                    .with_module(concat!("sdk-test-connect-disconnect", $suffix))
                    .with_language("rust")
                    // We test against multiple modules in different languages,
                    // and as of writing (pgoldman 2026-02-12),
                    // some of those languages have not yet been updated to make scheduled and lifecycle reducers
                    // private by default. As such, generating only public items results in different bindings
                    // depending on which module is the source.
                    .with_generate_private_items(true)
                    .with_bindings_dir("src/module_bindings")
                    .build()
                    .run();
            }

            #[test]
            fn caller_always_notified() {
                make_test("caller-always-notified").run();
            }

            #[test]
            // This test is currently broken due to our use of `with_generate_private_items(true)`.
            // Codegen will include private tables in the list of all tables,
            // meaning `subscribe_to_all_tables` will attempt to subscribe to private tables,
            // which will fail due to the client not being privileged.
            // TODO: once all modules are updated for `RawModuleDefV10`, disable generating private items in `make_test`,
            // and re-enable this test.
            // Alternatively, either split this test out into a separate module/client pair which runs only against V10 modules,
            // or mark every table in the `sdk-test` family of modules `public`.
            #[should_panic]
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

            // The Rust client variant of this test is currently under-synchronized:
            // it returns basically instantly after starting the connection.
            // It's also somewhat broken due to casing issues.
            // Re-enable this test once it is fixed and properly waiting for its results.
            #[test]
            #[ignore = "Flaky until test-client retains ignored connections or this test owns its connection lifetime"]
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

            #[test]
            fn sorted_uuids_insert() {
                make_test("sorted-uuids-insert").run();
            }
        }
    };
}

declare_tests_with_suffix!(rust, "");
declare_tests_with_suffix!(typescript, "-ts");
// TODO: migrate csharp to snake_case table names
declare_tests_with_suffix!(csharp, "-cs");
declare_tests_with_suffix!(cpp, "-cpp");

/// Tests of event table functionality, using <./event-table-client> and <../../../modules/sdk-test>.
///
/// These are separate from the existing client because as of writing (2026-02-07),
/// we do not have event table support in all of the module languages we have tested.
mod event_table_tests {
    use spacetimedb_testing::sdk::Test;

    const MODULE: &str = "sdk-test-event-table";
    const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/event-table-client");

    fn make_test(subcommand: &str) -> Test {
        super::platform_test_builder(CLIENT, Some(subcommand))
            .with_name(subcommand)
            .with_module(MODULE)
            .with_language("rust")
            .with_bindings_dir("src/module_bindings")
            .build()
    }

    #[test]
    fn event_table() {
        make_test("event-table").run();
    }

    #[test]
    fn multiple_events() {
        make_test("multiple-events").run();
    }

    #[test]
    fn events_dont_persist() {
        make_test("events-dont-persist").run();
    }
}

macro_rules! procedure_tests {
    ($mod_name:ident, $suffix:literal) => {
        mod $mod_name {
            //! Tests of procedure functionality, using <./procedure_client> and <../../../modules/sdk-test-procedure>.
            //!
            //! These are separate from the existing client and module because as of writing (pgoldman 2025-10-30),
            //! we do not have procedure support in all of the module languages we have tested.

            use spacetimedb_testing::sdk::Test;

            const MODULE: &str = concat!("sdk-test-procedure", $suffix);
            const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/procedure-client");

            fn make_test(subcommand: &str) -> Test {
                super::platform_test_builder(CLIENT, Some(subcommand))
                    .with_name(subcommand)
                    .with_module(MODULE)
                    .with_language("rust")
                    // We test against multiple modules in different languages,
                    // and as of writing (pgoldman 2026-02-12),
                    // some of those languages have not yet been updated to make scheduled and lifecycle reducers
                    // private by default. As such, generating only public items results in different bindings
                    // depending on which module is the source.
                    .with_generate_private_items(true)
                    .with_bindings_dir("src/module_bindings")
                    .build()
            }

            #[test]
            fn return_values() {
                make_test("procedure-return-values").run()
            }

            #[test]
            fn observe_panic() {
                make_test("procedure-observe-panic").run()
            }

            #[test]
            fn with_tx_commit() {
                make_test("insert-with-tx-commit").run()
            }

            #[test]
            fn with_tx_rollback() {
                make_test("insert-with-tx-rollback").run()
            }

            #[test]
            fn http_ok() {
                make_test("procedure-http-ok").run()
            }

            #[test]
            fn http_err() {
                make_test("procedure-http-err").run()
            }

            #[test]
            fn schedule_procedure() {
                make_test("schedule-procedure").run()
            }
        }
    };
}

procedure_tests!(rust_procedures, "");
procedure_tests!(typescript_procedures, "-ts");
procedure_tests!(cpp_procedures, "-cpp");

macro_rules! view_tests {
    ($mod_name:ident, $suffix:literal) => {
        mod $mod_name {
            use spacetimedb_testing::sdk::Test;

            const MODULE: &str = concat!("sdk-test-view", $suffix);
            const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/view-client");

            fn make_test(subcommand: &str) -> Test {
                super::platform_test_builder(CLIENT, Some(subcommand))
                    .with_name(subcommand)
                    .with_module(MODULE)
                    .with_language("rust")
                    // We test against multiple modules in different languages,
                    // and as of writing (pgoldman 2026-02-12),
                    // some of those languages have not yet been updated to make scheduled and lifecycle reducers
                    // private by default. As such, generating only public items results in different bindings
                    // depending on which module is the source.
                    .with_generate_private_items(true)
                    .with_bindings_dir("src/module_bindings")
                    .build()
            }

            #[test]
            fn subscribe_anonymous_view() {
                make_test("view-anonymous-subscribe").run()
            }

            #[test]
            fn subscribe_anonymous_view_query_builder() {
                make_test("view-anonymous-subscribe-with-query-builder").run()
            }

            #[test]
            fn subscribe_non_anonymous_view() {
                make_test("view-non-anonymous-subscribe").run()
            }

            #[test]
            fn subscribe_view_non_table_return() {
                make_test("view-non-table-return").run()
            }

            #[test]
            fn subscribe_view_non_table_query_builder_return() {
                make_test("view-non-table-query-builder-return").run()
            }

            #[test]
            fn subscription_updates_for_view() {
                make_test("view-subscription-update").run()
            }

            #[test]
            fn disconnect_does_not_break_sender_view_updates() {
                make_test("view-disconnect-does-not-break-sender-updates").run()
            }
        }
    };
}

view_tests!(rust_view, "");
view_tests!(cpp_view, "-cpp");

macro_rules! view_pk_tests {
    ($mod_name:ident, $suffix:literal) => {
        mod $mod_name {
            use spacetimedb_testing::sdk::Test;

            const MODULE: &str = concat!("sdk-test-view-pk", $suffix);
            const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/view-pk-client");

            fn make_test(subcommand: &str) -> Test {
                super::platform_test_builder(CLIENT, Some(subcommand))
                    .with_name(subcommand)
                    .with_module(MODULE)
                    .with_language("rust")
                    .with_bindings_dir("src/module_bindings")
                    .build()
            }

            #[test]
            fn query_builder_view_with_pk_on_update_callback() {
                make_test("view-pk-on-update").run()
            }

            #[test]
            fn query_builder_join_table_with_view_pk() {
                make_test("view-pk-join-query-builder").run()
            }

            #[test]
            fn query_builder_semijoin_two_sender_views_with_pk() {
                make_test("view-pk-semijoin-two-sender-views-query-builder").run()
            }
        }
    };
}

view_pk_tests!(rust_view_pk, "");
view_pk_tests!(csharp_view_pk, "-cs");
