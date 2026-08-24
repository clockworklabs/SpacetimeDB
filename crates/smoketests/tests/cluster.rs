// Cluster test binary entry point. These smoketests provide useful coverage
// against a cluster, but can also run against a local standalone server.
//
// We group the tests into one binary to avoid linking every source file as an
// independent integration test target.
mod cluster {
    mod add_remove_index;
    mod auto_inc;
    mod auto_migration;
    mod call;
    mod cli;
    mod client_connection_errors;
    mod column_defaults;
    mod confirmed_reads;
    mod connect_disconnect_from_cli;
    mod database_lock;
    mod delete_database;
    mod describe;
    mod dml;
    mod domains;
    mod fail_initial_publish;
    mod filtering;
    mod http_egress;
    mod http_routes;
    mod logs_level_filter;
    mod module_nested_op;
    mod modules;
    mod namespaces;
    mod new_user_flow;
    mod panic;
    mod permissions;
    mod publish_upgrade_prompt;
    mod quickstart;
    mod rls;
    mod schedule_reducer;
    mod sql;
    mod sql_connect_hook;
    mod templates;
    mod timestamp_route;
    mod views;
}
