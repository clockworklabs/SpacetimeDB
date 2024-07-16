use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;

extern crate core;

pub mod energy;
pub mod json;
pub mod sql;

pub static STDB_PATH: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from(std::env::var_os("STDB_PATH").expect("STDB_PATH must be set")));

pub fn stdb_path<S>(s: &S) -> PathBuf
where
    S: AsRef<Path> + ?Sized,
{
    STDB_PATH.join(s)
}

pub mod address {
    pub use spacetimedb_lib::Address;
}
pub mod auth;
pub mod db;
pub mod messages;
pub use spacetimedb_lib::Identity;
pub mod error;
pub use spacetimedb_lib::identity;
pub use spacetimedb_sats::hash;
pub mod callgrind_flag;
pub mod client;
pub mod config;
pub mod database_instance_context;
pub mod database_logger;
pub mod estimation;
pub mod execution_context;
pub mod host;
pub mod module_host_context;
pub mod sendgrid_controller;
pub mod startup;
pub mod subscription;
pub mod util;
pub mod vm;
pub mod worker_metrics;
