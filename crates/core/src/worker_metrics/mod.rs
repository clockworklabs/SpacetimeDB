use once_cell::sync::Lazy;
use prometheus::core::{Metric, MetricVec, MetricVecBuilder};
use prometheus::{Gauge, GaugeVec, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec};
use spacetimedb_lib::{Address, Hash, Identity};

#[macro_export]
macro_rules! metrics_group {
    ($(#[$attr:meta])* $type_vis:vis struct $type_name:ident {
        $(#[name = $name:ident] #[help = $help:expr] $(#[labels($($labels:ident: $labelty:ty),*)])? $vis:vis $field:ident: $ty:ident,)*
    }) => {
        $(#[$attr])*
        $type_vis struct $type_name {
            $($vis $field: $crate::metrics_group!(@fieldtype $field $ty $(($($labels)*))?),)*
        }
        $($crate::metrics_group!(@maketype $vis $field $ty $(($($labels: $labelty),*))?);)*
        impl $type_name {
            pub fn new() -> Self {
                Self {
                    $($field: $crate::make_collector!($crate::metrics_group!(@fieldtype $field $ty $(($($labels)*))?), stringify!($name), $help),)*
                }
            }
        }

        impl prometheus::core::Collector for $type_name {
            fn desc(&self) -> Vec<&prometheus::core::Desc> {
                $crate::worker_metrics::itertools::concat([ $(prometheus::core::Collector::desc(&self.$field)),* ])
            }

            fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
                $crate::worker_metrics::itertools::concat([ $(prometheus::core::Collector::collect(&self.$field)),* ])
            }
        }
        impl prometheus::core::Collector for &$type_name {
            fn desc(&self) -> Vec<&prometheus::core::Desc> {
                (**self).desc()
            }

            fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
                (**self).collect()
            }
        }
    };
    (@fieldtype $field:ident $ty:ident ($($labels:tt)*)) => { $crate::worker_metrics::paste! { [< $field:camel $ty >] } };
    (@fieldtype $field:ident $ty:ident) => { $ty };
    (@maketype $vis:vis $field:ident $ty:ident ($($labels:tt)*)) => {
        $crate::worker_metrics::paste! {
            $crate::metrics_vec!($vis [< $field:camel $ty >]: $ty($($labels)*));
        }
    };
    (@maketype $vis:vis $field:ident $ty:ident) => {};
}
pub use metrics_group;
#[doc(hidden)]
pub use {itertools, paste::paste};

metrics_group!(
    pub struct WorkerMetrics {
        #[name = spacetime_worker_connected_clients]
        #[help = "Number of clients connected to the worker."]
        pub connected_clients: IntGauge,

        #[name = spacetime_websocket_requests]
        #[help = "Number of websocket request messages"]
        #[labels(instance_id: u64, protocol: str)]
        pub websocket_requests: IntCounterVec,

        #[name = spacetime_websocket_request_msg_size]
        #[help = "The size of messages received on connected sessions"]
        #[labels(instance_id: u64, protocol: str)]
        pub websocket_request_msg_size: HistogramVec,

        #[name = spacetime_websocket_sent]
        #[help = "Number of websocket messages sent to client"]
        #[labels(identity: Identity)]
        pub websocket_sent: IntCounterVec,

        #[name = spacetime_websocket_sent_msg_size]
        #[help = "The size of messages sent to connected sessions"]
        #[labels(identity: Identity)]
        pub websocket_sent_msg_size: HistogramVec,

        #[name = spacetime_worker_process_cpu_usage]
        #[help = "CPU usage of the worker process."]
        pub process_cpu_usage: Gauge,

        #[name = spacetime_worker_transactions]
        #[help = "Number of reducer calls."]
        #[labels(database_address: Address, reducer_symbol: str)]
        pub reducer_count: IntCounterVec,

        #[name = spacetime_worker_module_tx_compute_time]
        #[help = "The time it takes to compute and commit after reducer execution."]
        #[labels(database_address: Address, reducer_symbol: str)]
        pub reducer_compute_time: HistogramVec,

        #[name = spacetime_worker_tx_size]
        #[help = "The size of committed bytes in the message log after reducer execution."]
        #[labels(database_address: Address, reducer_symbol: str)]
        pub reducer_write_size: HistogramVec,

        #[name = spacetime_worker_identity_energy_budget]
        #[help = "Node-level energy budget, per identity"]
        #[labels(identity: Identity, node: u64)]
        pub node_identity_energy_budget_gauge: GaugeVec,

        #[name = spacetime_instance_env_insert]
        #[help = "Time spent by reducers inserting rows (InstanceEnv::insert)"]
        #[labels(database_address: Address, table_id: u32)]
        pub instance_env_insert: HistogramVec,

        #[name = spacetime_instance_env_delete_eq]
        #[help = "Time spent by reducers deleting rows by eq (InstanceEnv::delete_eq)"]
        #[labels(database_address: Address, table_id: u32)]
        pub instance_env_delete_eq: HistogramVec,

        #[name = spacetime_worker_instance_operation_queue_length]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(identity: Identity, module_hash: Hash)]
        pub instance_queue_length: IntGaugeVec,
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

#[macro_export]
macro_rules! make_collector {
    ($ty:ty, $name:expr, $help:expr $(,)?) => {
        <$ty>::with_opts(prometheus::Opts::new($name, $help).into()).unwrap()
    };
    ($ty:ty, $name:expr, $help:expr, $labels:expr $(,)?) => {
        <$ty>::new(prometheus::Opts::new($name, $help).into(), $labels).unwrap()
    };
}
pub use make_collector;

#[macro_export]
macro_rules! metrics_vec {
    ($vis:vis $name:ident: $vecty:ident($($labels:ident: $labelty:ty),+ $(,)?)) => {
        #[derive(Clone)]
        $vis struct $name($vecty);
        impl $name {
            pub fn with_opts(opts: prometheus::Opts) -> prometheus::Result<Self> {
                $vecty::new(opts.into(), &[$(stringify!($labels)),+]).map(Self)
            }

            pub fn with_label_values(&self, $($labels: &$labelty),+) -> <$vecty as $crate::worker_metrics::ExtractMetricVecT>::M {
                use $crate::worker_metrics::AsPrometheusLabel as _;
                self.0.with_label_values(&[ $($labels.as_prometheus_str().as_ref()),+ ])
            }
        }

        impl prometheus::core::Collector for $name {
            fn desc(&self) -> Vec<&prometheus::core::Desc> {
                prometheus::core::Collector::desc(&self.0)
            }

            fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
                prometheus::core::Collector::collect(&self.0)
            }
        }
    };
}
pub use metrics_vec;

pub trait AsPrometheusLabel {
    type Str<'a>: AsRef<str> + 'a
    where
        Self: 'a;
    fn as_prometheus_str(&self) -> Self::Str<'_>;
}
impl<T: AsRef<str> + ?Sized> AsPrometheusLabel for &T {
    type Str<'a> = &'a str where Self: 'a;
    fn as_prometheus_str(&self) -> Self::Str<'_> {
        self.as_ref()
    }
}
macro_rules! impl_prometheusvalue_string {
    ($($x:ty),*) => {
        $(impl AsPrometheusLabel for $x {
            type Str<'a> = String;
            fn as_prometheus_str(&self) -> Self::Str<'_> {
                self.to_string()
            }
        })*
    }
}
impl_prometheusvalue_string!(Hash, Identity, Address, u8, u16, u32, u64, i8, i16, i32, i64);

#[doc(hidden)]
pub trait ExtractMetricVecT {
    type M: Metric;
}

impl<T: MetricVecBuilder> ExtractMetricVecT for MetricVec<T> {
    type M = T::M;
}
