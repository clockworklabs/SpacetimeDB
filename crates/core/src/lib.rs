use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;

extern crate core;

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

// to let us be incremental in updating all the references to what used to be individual lazy_statics
macro_rules! metrics_delegator {
    ($name:ident, $field:ident: $ty:ty) => {
        #[allow(non_camel_case_types)]
        pub struct $name {
            __private: (),
        }
        pub static $name: $name = $name { __private: () };
        impl std::ops::Deref for $name {
            type Target = $ty;
            fn deref(&self) -> &$ty {
                &METRICS.$field
            }
        }
    };
}

pub mod address {
    pub use spacetimedb_lib::Address;
}
pub mod auth;
pub mod db;
pub mod messages;
pub use spacetimedb_lib::Identity;
pub mod error;
pub mod hash;
pub use spacetimedb_lib::identity;
pub mod protobuf {
    pub use spacetimedb_client_api_messages::*;
}
pub mod client;
pub mod config;
pub mod control_db;
pub mod database_instance_context;
pub mod database_instance_context_controller;
pub mod database_logger;
pub mod host;
pub mod module_host_context;
pub mod object_db;
pub mod sendgrid_controller;
pub mod startup;
pub mod subscription;
pub mod util;
pub mod vm;
pub mod worker_metrics;
