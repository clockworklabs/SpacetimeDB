//! Compare the cost of the same team chat as a per-user view and as a scoped view,
//! as the number of subscribers on the team grows.
//!
//! Every subscriber is on the same team and subscribed to the view,
//! and one of them sends messages to the team.
//! For each message, this measures the latency of the reducer,
//! which includes refreshing the views and evaluating subscription updates,
//! and the time until every subscriber has received its update.
//! It also measures the latency of a subscriber switching teams and back.
//!
//! This is a benchmark rather than a test, so it is ignored by default.
//! Run it with:
//!
//! ```sh
//! cargo test --release -p spacetimedb-testing --test scoped_views_bench -- --ignored --nocapture
//! ```
//!
//! Set `SCOPED_VIEWS_BENCH_SUBSCRIBERS` to a comma-separated list to override the numbers of subscribers.

use spacetimedb::client::{
    ClientConfig, ClientConnection, ClientConnectionReceiver, OutboundMessage, Protocol, WsVersion,
};
use spacetimedb::host::FunctionArgs;
use spacetimedb::Identity;
use spacetimedb_client_api_messages::websocket::{common as ws_common, v1 as ws_v1, v2 as ws_v2};
use spacetimedb_lib::bsatn;
use spacetimedb_lib::sats::{product, u256, ProductValue};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, ModuleHandle, DEFAULT_CONFIG};
use std::time::{Duration, Instant};

/// The numbers of subscribers to measure, unless overridden by `SCOPED_VIEWS_BENCH_SUBSCRIBERS`.
const SUBSCRIBERS: &[usize] = &[1, 10, 50, 100, 250];
/// Messages sent before measuring, to warm up.
const WARMUP_MESSAGES: usize = 5;
/// Messages measured.
const MESSAGES: usize = 50;
/// Team switches measured.
const MOVES: usize = 10;
/// How long to wait for an update before giving up.
const RECV_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
enum ViewKind {
    PerUser,
    Scoped,
}

impl ViewKind {
    fn name(self) -> &'static str {
        match self {
            Self::PerUser => "per-user",
            Self::Scoped => "scoped",
        }
    }

    fn table(self) -> &'static str {
        match self {
            Self::PerUser => "team_chat_per_user",
            Self::Scoped => "team_chat_scoped",
        }
    }
}

struct Measurement {
    /// Mean latency of sending a message, including refreshing views and evaluating updates.
    send: Duration,
    /// Mean time from sending a message until every subscriber received its update.
    delivered: Duration,
    /// Mean latency of a subscriber switching teams.
    moved: Duration,
}

struct Subscriber {
    client: ClientConnection,
    rx: ClientConnectionReceiver,
}

fn v2_config() -> ClientConfig {
    ClientConfig {
        protocol: Protocol::Binary,
        version: WsVersion::V2,
        compression: ws_common::Compression::None,
        tx_update_full: true,
        confirmed_reads: false,
    }
}

async fn call(client: &ClientConnection, reducer: &str, args: ProductValue) {
    let args = FunctionArgs::Bsatn(bsatn::to_vec(&args).unwrap().into());
    // Don't notify the caller separately, so that it receives updates like every other subscriber.
    let result = client
        .call_reducer(
            reducer,
            args,
            0,
            Instant::now(),
            ws_v1::CallReducerFlags::NoSuccessNotify,
        )
        .await
        .unwrap();
    assert!(result.is_ok(), "reducer {reducer} failed");
}

/// Receive the transaction update for the next transaction from `rx`.
async fn recv_update(rx: &mut ClientConnectionReceiver) {
    let mut buf = Vec::with_capacity(1);
    let received = tokio::time::timeout(RECV_TIMEOUT, rx.recv_many(&mut buf, 1))
        .await
        .expect("timed out waiting for an update");
    assert_eq!(received, 1, "client receiver closed");
    match buf.remove(0) {
        OutboundMessage::V2(ws_v2::ServerMessage::TransactionUpdate(_)) => {}
        message => panic!("expected a transaction update, got {message:?}"),
    }
}

/// Discard any messages already sent to `rx`.
async fn drain(rx: &mut ClientConnectionReceiver) {
    let mut buf = Vec::new();
    while let Ok(n) = tokio::time::timeout(Duration::from_millis(20), rx.recv_many(&mut buf, 4096)).await {
        if n == 0 {
            break;
        }
        buf.clear();
    }
}

async fn measure(module: &ModuleHandle, view: ViewKind, num_subscribers: usize) -> Measurement {
    // Connect every subscriber, put them on team 1, and subscribe them to the view.
    let mut subscribers = Vec::with_capacity(num_subscribers);
    for i in 0..num_subscribers {
        let identity = Identity::from_u256(u256::from(i as u128 + 1));
        let (client, rx) = module.connect(identity, v2_config());
        call(&client, "join", product![1u64]).await;
        let subscribe = ws_v2::Subscribe {
            request_id: 0,
            query_set_id: ws_v2::QuerySetId::new(0),
            query_strings: [format!("SELECT * FROM {}", view.table()).into()].into(),
        };
        client.subscribe_v2(subscribe, Instant::now()).await.unwrap();
        subscribers.push(Subscriber { client, rx });
    }
    for subscriber in &mut subscribers {
        drain(&mut subscriber.rx).await;
    }

    // Send messages to the team.
    let (mut send, mut delivered) = (Duration::ZERO, Duration::ZERO);
    for i in 0..WARMUP_MESSAGES + MESSAGES {
        let start = Instant::now();
        call(&subscribers[0].client, "send", product![1u64, format!("message {i}")]).await;
        let sent = start.elapsed();
        for subscriber in &mut subscribers {
            recv_update(&mut subscriber.rx).await;
        }
        if i >= WARMUP_MESSAGES {
            send += sent;
            delivered += start.elapsed();
        }
    }

    // Move the last subscriber to another team and back.
    let mover = subscribers.last_mut().unwrap();
    let mut moved = Duration::ZERO;
    for i in 0..MOVES {
        let team = if i % 2 == 0 { 2u64 } else { 1u64 };
        let start = Instant::now();
        call(&mover.client, "set_team", product![team]).await;
        moved += start.elapsed();
        recv_update(&mut mover.rx).await;
    }

    Measurement {
        send: send / MESSAGES as u32,
        delivered: delivered / MESSAGES as u32,
        moved: moved / MOVES as u32,
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "benchmark; run explicitly with --ignored"]
fn scoped_views_bench() {
    let module = CompiledModule::compile("scoped-views-bench", CompilationMode::Release);

    let subscribers = match std::env::var("SCOPED_VIEWS_BENCH_SUBSCRIBERS") {
        Ok(list) => list.split(',').map(|n| n.trim().parse().unwrap()).collect(),
        Err(_) => SUBSCRIBERS.to_vec(),
    };

    let mut results = Vec::new();
    for num_subscribers in subscribers {
        for view in [ViewKind::PerUser, ViewKind::Scoped] {
            // Measure each configuration against a fresh database.
            let (tx, rx) = std::sync::mpsc::channel();
            module.with_module_async(DEFAULT_CONFIG, |module| async move {
                tx.send(measure(&module, view, num_subscribers).await).unwrap();
            });
            let measurement = rx.recv().unwrap();
            eprintln!(
                "{num_subscribers:>4} subscribers, {:>8}: send {}, delivered {}, move {}",
                view.name(),
                format_duration(measurement.send),
                format_duration(measurement.delivered),
                format_duration(measurement.moved),
            );
            results.push((num_subscribers, view, measurement));
        }
    }

    println!();
    println!("| Subscribers | View | Send latency | Delivered to all | Switch team |");
    println!("|---:|---|---:|---:|---:|");
    for (num_subscribers, view, measurement) in results {
        println!(
            "| {num_subscribers} | {} | {} | {} | {} |",
            view.name(),
            format_duration(measurement.send),
            format_duration(measurement.delivered),
            format_duration(measurement.moved),
        );
    }
}
