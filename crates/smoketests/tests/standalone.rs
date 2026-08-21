// Standalone test binary entry point.
//
// These smoketests are assigned to standalone coverage. Some require control
// of a local SpacetimeDB server; others simply provide no additional value when
// repeated against a cluster. Tests that require local server control keep
// `require_local_server!()` as a defensive check.
mod standalone {
    mod auto_migration;
    mod change_host_type;
    mod cli;
    mod client_connection_errors;
    mod create_project;
    mod csharp_aot_module;
    mod csharp_module;
    mod default_module_clippy;
    mod detect_wasm_bindgen;
    mod http_egress;
    mod pg_wire;
    mod restart;
    mod servers;
    mod templates;
    mod typescript_index_source_name;
    mod views;
}
