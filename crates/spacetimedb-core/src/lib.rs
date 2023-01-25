extern crate core;

pub mod json;
pub mod sql;
pub mod websocket;

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
#[allow(clippy::derive_partial_eq_without_eq)]
pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/protobuf.rs"));
}
pub mod client;
pub mod control_db;
pub mod database_instance_context_controller;
pub mod database_logger;
pub mod host;
pub mod module_subscription_actor;
pub mod object_db;
pub mod startup;
pub mod util;
pub mod worker_database_instance;
pub mod worker_metrics;
