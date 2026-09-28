use spacetimedb_smoketests::{random_string, require_local_server, Smoketest};

#[test]
fn test_typescript_add_optional_columns() {
    require_local_server!();

    let mut test = Smoketest::builder().autopublish(false).build();
    let module_name = format!("typescript-add-optional-columns-{}", random_string());

    test.use_precompiled_module("typescript-add-optional-columns-v1");
    let database_identity = test.publish().name(&module_name).run().unwrap();

    test.call("insert_user", &["Alice", "alice@example.com"]).unwrap();

    test.restart_server();

    test.use_precompiled_module("typescript-add-optional-columns-v2");
    test.publish().name(&database_identity).run().unwrap();

    test.call("find_user_by_email", &["alice@example.com"]).unwrap();
    test.call("find_users_by_active_status", &["false"]).unwrap();
}

#[test]
fn test_typescript_change_index_source_name() {
    require_local_server!();

    let mut test = Smoketest::builder().autopublish(false).build();
    let module_name = format!("typescript-change-source-name-{}", random_string());

    test.use_precompiled_module("typescript-add-optional-columns-v1");
    let database_identity = test.publish().name(&module_name).run().unwrap();

    test.call("insert_user", &["Alice", "alice@example.com"]).unwrap();

    test.use_precompiled_module("typescript-change-source-name-v2");
    test.publish().name(&database_identity).run().unwrap();

    test.call("find_user_by_email", &["alice@example.com"]).unwrap();
}
