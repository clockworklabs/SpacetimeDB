mod auto_migration;
mod cli;
mod client_connection_errors;
mod http_egress;
mod servers;
mod views;

#[path = "../smoketests/change_host_type.rs"]
mod change_host_type;
#[path = "../smoketests/pg_wire.rs"]
mod pg_wire;
#[path = "../smoketests/restart.rs"]
mod restart;
#[path = "../smoketests/typescript_index_source_name.rs"]
mod typescript_index_source_name;
