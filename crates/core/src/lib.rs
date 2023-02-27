use std::path::{Path, PathBuf};

extern crate core;

pub mod json;
pub mod sql;
pub mod websocket;

lazy_static::lazy_static! {
    pub static ref STDB_PATH: String = std::env::var("STDB_PATH").unwrap_or("/stdb".to_owned());
}

pub fn stdb_path<S>(s: &S) -> PathBuf
where
    S: AsRef<Path> + ?Sized,
{
    Path::new(&STDB_PATH.as_str()).join(s)
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

pub mod address;
pub mod auth;
pub mod db;
pub mod error;
pub mod hash;
pub mod identity;
#[allow(clippy::derive_partial_eq_without_eq)]
pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/protobuf.rs"));
}
pub mod client;
pub mod control_db;
pub mod database_instance_context_controller;
pub mod database_logger;
pub mod host;
pub mod object_db;
pub mod sendgrid_controller;
pub mod startup;
pub mod subscription;
pub mod util;
pub mod worker_database_instance;
pub mod worker_metrics;
