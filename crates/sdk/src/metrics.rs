use once_cell::sync::Lazy;
use prometheus::{HistogramVec, IntCounterVec};
use spacetimedb_lib::ConnectionId;
use spacetimedb_metrics::metrics_group;

metrics_group!(
    pub struct ClientMetrics {
        #[name = spacetime_client_received_total]
        #[help = "The cumulative number of received websocket messages"]
        #[labels(db: Box<str>, connection_id: ConnectionId)]
        pub websocket_received: IntCounterVec,

        #[name = spacetime_client_received_msg_size]
        #[help = "The size of received websocket messages"]
        #[labels(db: Box<str>, connection_id: ConnectionId)]
        pub websocket_received_msg_size: HistogramVec,
    }
);

pub static CLIENT_METRICS: Lazy<ClientMetrics> = Lazy::new(ClientMetrics::new);
