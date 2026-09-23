//! Compare the cost of the same team chat as a per-user view and as a scoped view,
//! as the numbers of teams and of subscribers per team grow.
//!
//! Each configuration has some number of teams, each with some number of players,
//! all subscribed to the view, and each with the same chat history to start.
//! Messages are sent to the teams in turn, each by a player on the team.
//! For each message, this measures the latency of the reducer,
//! which includes refreshing the views and evaluating subscription updates,
//! and the time until every player on the team has received its update.
//! It checks that no player receives an update for another team's message.
//! It also measures the latency of a player switching between two populated teams,
//! and counts the rows materialized for the view.
//!
//! This is a benchmark rather than a test, so it is ignored by default.
//! Run it with:
//!
//! ```sh
//! cargo test --release -p spacetimedb-testing --test scoped_views_bench -- --ignored --nocapture
//! ```
//!
//! Set `SCOPED_VIEWS_BENCH_CONFIGS` to a comma-separated list of `{teams}x{players per team}`,
//! e.g. `20x5,1x250`, to override the configurations measured.

use spacetimedb::client::{
    ClientConfig, ClientConnection, ClientConnectionReceiver, OutboundMessage, Protocol, WsVersion,
};
use spacetimedb::host::FunctionArgs;
use spacetimedb::Identity;
use spacetimedb_client_api_messages::websocket::{common as ws_common, v1 as ws_v1, v2 as ws_v2};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView as _;
use spacetimedb_lib::bsatn;
use spacetimedb_lib::sats::{product, u256, ProductValue};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, ModuleHandle, DEFAULT_CONFIG};
use std::time::{Duration, Instant};

/// The configurations to measure, as `(teams, players per team)`,
/// unless overridden by `SCOPED_VIEWS_BENCH_CONFIGS`.
const CONFIGS: &[(usize, usize)] = &[
    // One team, growing.
    (1, 1),
    (1, 10),
    (1, 50),
    (1, 100),
    (1, 250),
    // Many teams.
    (20, 5),
    (20, 25),
    (20, 50),
    (50, 20),
    (100, 10),
    // The worst case for scoped views: every player alone in their scope.
    (100, 1),
    (500, 1),
    (1000, 1),
];
/// Messages in each team's chat before any player subscribes,
/// so that every configuration measures teams with the same history.
const HISTORY_PER_TEAM: usize = 100;
/// Messages sent before measuring, to warm up.
const WARMUP_MESSAGES: usize = 10;
/// Messages measured.
const MESSAGES: usize = 100;
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
    /// Mean latency of subscribing to the view, including materializing it for the subscriber.
    subscribe: Duration,
    /// Mean latency of sending a message, including refreshing views and evaluating updates.
    send: Duration,
    /// Mean time from sending a message until every player on the team received its update.
    delivered: Duration,
    /// Mean latency of a player switching between two populated teams.
    moved: Duration,
    /// Rows materialized for the view at the end.
    view_rows: u64,
}

struct Player {
    team: u64,
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

/// Wait until `rx` receives the rows of its subscription.
async fn recv_subscribe_applied(rx: &mut ClientConnectionReceiver) {
    let mut buf = Vec::with_capacity(1);
    loop {
        let received = tokio::time::timeout(RECV_TIMEOUT, rx.recv_many(&mut buf, 1))
            .await
            .expect("timed out waiting for the subscription to apply");
        assert_eq!(received, 1, "client receiver closed");
        match buf.remove(0) {
            OutboundMessage::V2(ws_v2::ServerMessage::SubscribeApplied(_)) => return,
            OutboundMessage::V2(ws_v2::ServerMessage::SubscriptionError(err)) => panic!("subscription failed: {err:?}"),
            _ => {}
        }
    }
}

/// Discard any messages already sent to `rx`, returning how many there were.
async fn drain(rx: &mut ClientConnectionReceiver) -> usize {
    let mut buf = Vec::new();
    let mut drained = 0;
    while let Ok(n) = tokio::time::timeout(Duration::from_millis(20), rx.recv_many(&mut buf, 4096)).await {
        if n == 0 {
            break;
        }
        drained += n;
        buf.clear();
    }
    drained
}

/// Count the rows materialized for `view`, across all of its instances.
fn count_view_rows(module: &ModuleHandle, view: ViewKind) -> u64 {
    let db = module.client.module().relational_db().clone();
    db.with_read_only(Workload::Internal, |tx| {
        let table_id = db.table_id_from_name(tx, view.table()).unwrap().unwrap();
        tx.table_row_count(table_id).unwrap()
    })
}

async fn measure(module: &ModuleHandle, view: ViewKind, teams: usize, players_per_team: usize) -> Measurement {
    // Give every team the same history.
    for team in 0..teams as u64 {
        for i in 0..HISTORY_PER_TEAM {
            call(&module.client, "send", product![team, format!("history {i}")]).await;
        }
    }

    // Connect every player, put them on their team, and subscribe them to the view.
    // Player `i` is on team `i % teams`.
    let mut players = Vec::with_capacity(teams * players_per_team);
    let mut subscribe = Duration::ZERO;
    for i in 0..teams * players_per_team {
        let team = (i % teams) as u64;
        let identity = Identity::from_u256(u256::from(i as u128 + 1));
        let (client, rx) = module.connect(identity, v2_config());
        call(&client, "join", product![team]).await;
        let subscribe_request = ws_v2::Subscribe {
            request_id: 0,
            query_set_id: ws_v2::QuerySetId::new(0),
            query_strings: [format!("SELECT * FROM {}", view.table()).into()].into(),
        };
        let mut rx = rx;
        let start = Instant::now();
        client.subscribe_v2(subscribe_request, Instant::now()).await.unwrap();
        recv_subscribe_applied(&mut rx).await;
        subscribe += start.elapsed();
        players.push(Player { team, client, rx });
    }
    for player in &mut players {
        drain(&mut player.rx).await;
    }

    // Send messages to each team in turn, each from a player on the team.
    let (mut send, mut delivered) = (Duration::ZERO, Duration::ZERO);
    for i in 0..WARMUP_MESSAGES + MESSAGES {
        let team = i % teams;
        let start = Instant::now();
        call(
            &players[team].client,
            "send",
            product![team as u64, format!("message {i}")],
        )
        .await;
        let sent = start.elapsed();
        for player in players.iter_mut().filter(|player| player.team == team as u64) {
            recv_update(&mut player.rx).await;
        }
        if i >= WARMUP_MESSAGES {
            send += sent;
            delivered += start.elapsed();
        }
    }

    // No player may receive an update for another team's message.
    for player in &mut players {
        assert_eq!(
            drain(&mut player.rx).await,
            0,
            "a player received another team's update"
        );
    }

    // Move a player between two populated teams, or to an empty team if there is only one.
    let mover = players.last_mut().unwrap();
    let home = mover.team;
    let away = (home + 1) % teams.max(2) as u64;
    let mut moved = Duration::ZERO;
    for i in 0..MOVES {
        let team = if i % 2 == 0 { away } else { home };
        let start = Instant::now();
        call(&mover.client, "set_team", product![team]).await;
        moved += start.elapsed();
        recv_update(&mut mover.rx).await;
    }

    Measurement {
        subscribe: subscribe / (teams * players_per_team) as u32,
        send: send / MESSAGES as u32,
        delivered: delivered / MESSAGES as u32,
        moved: moved / MOVES as u32,
        view_rows: count_view_rows(module, view),
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "benchmark; run explicitly with --ignored"]
fn scoped_views_bench() {
    let module = CompiledModule::compile("scoped-views-bench", CompilationMode::Release);

    let configs = match std::env::var("SCOPED_VIEWS_BENCH_CONFIGS") {
        Ok(list) => list
            .split(',')
            .map(|config| {
                let (teams, players) = config.trim().split_once('x').expect("expected {teams}x{players}");
                (teams.parse().unwrap(), players.parse().unwrap())
            })
            .collect(),
        Err(_) => CONFIGS.to_vec(),
    };

    let mut results = Vec::new();
    for (teams, players_per_team) in configs {
        for view in [ViewKind::PerUser, ViewKind::Scoped] {
            // Measure each configuration against a fresh database.
            let (tx, rx) = std::sync::mpsc::channel();
            module.with_module_async(DEFAULT_CONFIG, |module| async move {
                tx.send(measure(&module, view, teams, players_per_team).await).unwrap();
            });
            let measurement = rx.recv().unwrap();
            eprintln!(
                "{teams:>4} teams x {players_per_team:>3} players, {:>8}: subscribe {}, send {}, delivered {}, move {}, view rows {}",
                view.name(),
                format_duration(measurement.subscribe),
                format_duration(measurement.send),
                format_duration(measurement.delivered),
                format_duration(measurement.moved),
                measurement.view_rows,
            );
            results.push((teams, players_per_team, view, measurement));
        }
    }

    println!();
    println!(
        "| Teams | Players per team | Subscribers | View | Subscribe | Send latency | Delivered to team | Switch team | View rows |"
    );
    println!("|---:|---:|---:|---|---:|---:|---:|---:|---:|");
    for (teams, players_per_team, view, measurement) in results {
        println!(
            "| {teams} | {players_per_team} | {} | {} | {} | {} | {} | {} | {} |",
            teams * players_per_team,
            view.name(),
            format_duration(measurement.subscribe),
            format_duration(measurement.send),
            format_duration(measurement.delivered),
            format_duration(measurement.moved),
            measurement.view_rows,
        );
    }
}
