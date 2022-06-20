use lazy_static::lazy_static;
use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntGauge, Registry,
};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // pub static ref INCOMING_HTTP_REQUESTS: IntCounter =
    //     IntCounter::new("incoming_requests", "Incoming Requests").expect("metric can be created");
    // pub static ref RESPONSE_CODE_COLLECTOR: IntCounterVec = IntCounterVec::new(
    //     Opts::new("response_code", "Response Codes"),
    //     &["env", "statuscode", "type"]
    // )
    // .expect("metric can be created");

    pub static ref TX_COUNT: IntCounter =
        IntCounter::new("transactions", "Transactions").expect("metric can be created");

    pub static ref CONNECTED_GAME_CLIENTS: IntGauge =
        IntGauge::new("connected_game_clients", "Connected Game Clients").expect("metric can be created");

    pub static ref TX_LATENCY_COLLECTOR: HistogramVec = HistogramVec::new(
        HistogramOpts::new("transaction_latency", "Transaction Latencies"),
        &["request_type"]
    )
    .expect("metric can be created");
    
    pub static ref TX_SIZE_COLLECTOR: HistogramVec = HistogramVec::new(
        HistogramOpts::new("transaction_size", "Transaction Sizes"),
        &["request_type"]
    )
    .expect("metric can be created");
}

pub fn register_custom_metrics() {
    REGISTRY
        .register(Box::new(TX_COUNT.clone()))
        .expect("collector can be registered");

    REGISTRY
        .register(Box::new(CONNECTED_GAME_CLIENTS.clone()))
        .expect("collector can be registered");

    REGISTRY
        .register(Box::new(TX_LATENCY_COLLECTOR.clone()))
        .expect("collector can be registered");
    
    REGISTRY
        .register(Box::new(TX_SIZE_COLLECTOR.clone()))
        .expect("collector can be registered");
}