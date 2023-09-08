use once_cell::sync::Lazy;
use prometheus::{
    core::Collector, proto::MetricFamily, Gauge, GaugeVec, HistogramVec, IntCounterVec, IntGauge, Registry,
};

#[macro_export]
macro_rules! metrics_group {
    ($(#[$attr:meta])* $type_vis:vis struct $type_name:ident {
        $(#[name = $name:ident] #[help = $help:expr] $(#[labels($($labels:ident),*)])? $vis:vis $field:ident: $ty:ident,)*
    }) => {
        $(#[$attr])*
        $type_vis struct $type_name {
            pub registry: prometheus::Registry,
            $($vis $field: $ty,)*
        }
        impl $type_name {
            pub fn new() -> Self {
                let registry = prometheus::Registry::new();
                Self {
                    $($field: $crate::worker_metrics::register(
                            &registry,
                            $crate::make_collector!($ty, stringify!($name), $help, $(&[$(stringify!($labels)),*])?)
                    ),)*
                    registry,
                }
            }
        }
    };
}
pub use metrics_group;

pub fn gather_multiple<const N: usize>(registries: [&Registry; N]) -> Vec<MetricFamily> {
    registries.into_iter().flat_map(Registry::gather).collect()
}

metrics_group!(
    pub struct WorkerMetrics {
        #[name = spacetime_worker_connected_clients]
        #[help = "Number of clients connected to the worker."]
        pub connected_clients: IntGauge,

        #[name = spacetime_websocket_requests]
        #[help = "Number of websocket request messages"]
        #[labels(instance_id, protocol)]
        pub websocket_requests: IntCounterVec,

        #[name = spacetime_websocket_request_msg_size]
        #[help = "The size of messages received on connected sessions"]
        #[labels(instance_id, protocol)]
        pub websocket_request_msg_size: HistogramVec,

        #[name = spacetime_websocket_sent]
        #[help = "Number of websocket messages sent to client"]
        #[labels(identity)]
        pub websocket_sent: IntCounterVec,

        #[name = spacetime_websocket_sent_msg_size]
        #[help = "The size of messages sent to connected sessions"]
        #[labels(identity)]
        pub websocket_sent_msg_size: HistogramVec,

        #[name = spacetime_worker_process_cpu_usage]
        #[help = "CPU usage of the worker process."]
        pub process_cpu_usage: Gauge,

        #[name = spacetime_worker_transactions]
        #[help = "Number of reducer calls."]
        #[labels(database_address, reducer_symbol)]
        pub reducer_count: IntCounterVec,

        #[name = spacetime_worker_module_tx_compute_time]
        #[help = "The time it takes to compute and commit after reducer execution."]
        #[labels(database_address, reducer_symbol)]
        pub reducer_compute_time: HistogramVec,

        #[name = spacetime_worker_tx_size]
        #[help = "The size of committed bytes in the message log after reducer execution."]
        #[labels(database_address, reducer_symbol)]
        pub reducer_write_size: HistogramVec,

        #[name = spacetime_worker_identity_energy_budget]
        #[help = "Node-level energy budget, per identity"]
        #[labels(identity, node)]
        pub node_identity_energy_budget_gauge: GaugeVec,

        #[name = spacetime_instance_env_insert]
        #[help = "Time spent by reducers inserting rows (InstanceEnv::insert)"]
        #[labels(database_address, table_id)]
        pub instance_env_insert: HistogramVec,

        #[name = spacetime_instance_env_delete_eq]
        #[help = "Time spent by reducers deleting rows by eq (InstanceEnv::delete_eq)"]
        #[labels(database_address, table_id)]
        pub instance_env_delete_eq: HistogramVec,
        // #[name = spacetime_instance_env_delete_pk]
        // #[help = "Time spent by reducers deleting rows by pk (InstanceEnv::delete_pk)"]
        // #[labels(database_address, table_id)]
        // pub instance_env_delete_pk: HistogramVec,

        // #[name = spacetime_instance_env_delete_value]
        // #[help = "Time spent by reducers deleting rows (InstanceEnv::delete_value)"]
        // #[labels(database_address, table_id)]
        // pub instance_env_delete_value: HistogramVec,

        // #[name = spacetime_instance_env_delete_range]
        // #[help = "Time spent by reducers deleting rows ranges eq (InstanceEnv::delete_range)"]
        // #[labels(database_address, table_id)]
        // pub instance_env_delete_range: HistogramVec,
    }
);

pub static WORKER_METRICS: Lazy<WorkerMetrics> = Lazy::new(WorkerMetrics::new);

#[track_caller]
pub(crate) fn register<C: Collector + Clone + 'static>(registry: &Registry, collector: C) -> C {
    registry.register(Box::new(collector.clone())).unwrap();
    collector
}

#[macro_export]
macro_rules! make_collector {
    (Histogram, $($args:expr),+ $(,)?) => {
        prometheus::Histogram::with_opts(prometheus::HistogramOpts::new($($args),+)).unwrap()
    };
    (HistogramVec, $($args:expr),+ $(,)?) => { $crate::make_collector!(@vec HistogramVec, HistogramOpts, $($args),+) };
    (IntCounterVec, $($args:expr),+ $(,)?) => { $crate::make_collector!(@vec IntCounterVec, Opts, $($args),+) };
    (GaugeVec, $($args:expr),+ $(,)?) => { $crate::make_collector!(@vec GaugeVec, Opts, $($args),+) };
    ($ty:ident, $($args:expr),+ $(,)?) => {
        prometheus::$ty::new($($args),+).unwrap()
    };
    (@vec $ty:ident, $opts:ident, $n:expr, $h:expr, $($args:expr),*) => {
        prometheus::$ty::new(prometheus::$opts::new($n, $h), $($args),*).unwrap()
    };
}
pub use make_collector;

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
pub(crate) use metrics_delegator;

use WORKER_METRICS as METRICS;
metrics_delegator!(REGISTRY, registry: Registry);
metrics_delegator!(CONNECTED_CLIENTS, connected_clients: IntGauge);
metrics_delegator!(WEBSOCKET_REQUESTS, websocket_requests: IntCounterVec);
metrics_delegator!(WEBSOCKET_REQUEST_MSG_SIZE, websocket_request_msg_size: HistogramVec);
metrics_delegator!(WEBSOCKET_SENT, websocket_sent: IntCounterVec);
metrics_delegator!(WEBSOCKET_SENT_MSG_SIZE, websocket_sent_msg_size: HistogramVec);
metrics_delegator!(PROCESS_CPU_USAGE, process_cpu_usage: Gauge);
metrics_delegator!(REDUCER_COUNT, reducer_count: IntCounterVec);
metrics_delegator!(REDUCER_COMPUTE_TIME, reducer_compute_time: HistogramVec);
metrics_delegator!(REDUCER_WRITE_SIZE, reducer_write_size: HistogramVec);
metrics_delegator!(
    NODE_IDENTITY_ENERGY_BUDGET_GAUGE,
    node_identity_energy_budget_gauge: GaugeVec
);
metrics_delegator!(INSTANCE_ENV_INSERT, instance_env_insert: HistogramVec);
// metrics_delegator!(INSTANCE_ENV_DELETE_PK, instance_env_delete_pk: HistogramVec);
// metrics_delegator!(INSTANCE_ENV_DELETE_VALUE, instance_env_delete_value: HistogramVec);
metrics_delegator!(INSTANCE_ENV_DELETE_BY_COL_EQ, instance_env_delete_eq: HistogramVec);
//metrics_delegator!(INSTANCE_ENV_DELETE_RANGE, instance_env_delete_range: HistogramVec);
