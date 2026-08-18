mod auto_migration;
mod cli;
mod client_connection_errors;
mod http_egress;
mod servers;
mod views;

#[path = "../smoketests/change_host_type.rs"]
mod change_host_type;
#[path = "../smoketests/create_project.rs"]
mod create_project;
#[path = "../smoketests/csharp_aot_module.rs"]
mod csharp_aot_module;
#[path = "../smoketests/csharp_module.rs"]
mod csharp_module;
#[path = "../smoketests/default_module_clippy.rs"]
mod default_module_clippy;
#[path = "../smoketests/detect_wasm_bindgen.rs"]
mod detect_wasm_bindgen;
#[path = "../smoketests/pg_wire.rs"]
mod pg_wire;
#[path = "../smoketests/restart.rs"]
mod restart;
#[path = "template_metadata.rs"]
mod templates;
#[path = "../smoketests/typescript_index_source_name.rs"]
mod typescript_index_source_name;
