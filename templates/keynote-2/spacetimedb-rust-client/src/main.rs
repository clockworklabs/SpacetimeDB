mod websocket;

use crate::websocket::{Recv, Send, ServerMessage, WsParams};
use clap::{Args, Parser, Subcommand};
use core::sync::atomic::{AtomicU64, Ordering};
use humantime::{format_duration, parse_duration};
use rand::{SeedableRng as _, distr::Distribution, rngs::SmallRng};
use rand_distr::Zipf;
use spacetimedb_client_api_messages::websocket::v1::{CallReducer, CallReducerFlags, ClientMessage, Compression};
use spacetimedb_lib::bsatn;
use std::path::Path;
use std::sync::Arc;
use std::{fs, thread};
use std::time::Instant;
use tokio::runtime::{self, Handle, Runtime};
use tokio::task::JoinHandle;

const LOCALHOST: &str = "http://localhost:3000";
const MODULE: &str = "sim";

const DURATION: &str = "5s";
const WARMUP_DURATION: &str = "5s";
const ALPHA: f32 = 0.5;
const CONNECTIONS: usize = 10;
const INIT_BALANCE: i64 = 1_000_000;
const AMOUNT: u32 = 1;
const ACCOUNTS: u32 = 100_000;
const CONFIRMED_READS: bool = false;
// Max inflight reducer calls imposed by the server.
const MAX_INFLIGHT_REDUCERS: u64 = 16384;

// When called from within an async context, return a handle to it (and no
// `Runtime`), otherwise create a fresh `Runtime` and return it along with a
// handle to it.
fn enter_or_create_runtime(connections: usize) -> (Option<Runtime>, runtime::Handle) {
    match runtime::Handle::try_current() {
        Err(e) if e.is_missing_context() => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(connections)
                .thread_name("spacetimedb-background-connection")
                .build()
                .unwrap();
            let handle = rt.handle().clone();

            (Some(rt), handle)
        }
        Ok(handle) => (None, handle),
        Err(_) => unimplemented!(),
    }
}

async fn init_conn(cli: &Common, handle: &Handle) -> (JoinHandle<()>, Recv, Send) {
    let server: &str = &*cli.server;
    let uri = server.try_into().unwrap();
    let params = WsParams {
        compression: Compression::None,
        light: true,
        confirmed: cli.confirmed_reads.into(),
    };

    let conn = websocket::WsConnection::connect(uri, &cli.module, None, None, params)
        .await
        .unwrap();

    let (jh, mut rx, tx) = conn.spawn_message_loop(&handle);

    let init = rx.recv().await.unwrap();
    assert_eq!(init, ServerMessage::IdentityToken);

    (jh, rx, tx)
}

fn pick_two_distinct(mut pick: impl FnMut() -> u32, max_spins: usize) -> (u32, u32) {
    let a = pick();
    let mut b = pick();

    let mut spins = 0;
    while a == b && spins < max_spins {
        b = pick();
        spins += 1;
    }

    (a, b)
}

fn make_transfers(accounts: u32, alpha: f32) -> Vec<(u32, u32)> {
    let dist = Zipf::new(accounts as f32, alpha).unwrap();
    let mut rng = SmallRng::seed_from_u64(0x12345678);
    (0..10_000_000)
        .filter_map(|_| {
            let (from, to) = pick_two_distinct(|| dist.sample(&mut rng) as u32, 32);
            if from >= accounts || to >= accounts || from == to {
                None
            } else {
                Some((from, to))
            }
        })
        .collect()
}

fn seed(cli: &Common, seed: &Seed) {
    let (_runtime, handle) = enter_or_create_runtime(1);
    let (jh, mut rx, tx) = handle.block_on(init_conn(&cli, &handle));

    let args = (cli.accounts, seed.initial_balance);
    let args = bsatn::to_vec(&args).unwrap().into();
    tx.send(ClientMessage::CallReducer(CallReducer {
        reducer: "seed".into(),
        args,
        request_id: 0,
        flags: CallReducerFlags::FullUpdate,
    }))
    .unwrap();

    let reply = rx.blocking_recv().unwrap();
    assert_eq!(reply, ServerMessage::TransactionUpdate);

    if !cli.quiet {
        println!("done seeding");
    }

    jh.abort();
}

fn bench(cli: &Common, bench: &Bench) {
    let (_runtime, handle) = enter_or_create_runtime(bench.connections);

    // Dump some config parameters.
    let alpha = bench.alpha;
    let accounts = cli.accounts;
    let amount = bench.amount;
    if !cli.quiet {
        println!("Benchmark parameters:");
        println!("alpha={alpha}, amount = {amount}, accounts = {accounts}");
        println!("max inflight reducers = {}", bench.max_inflight_reducers);
        println!();
    }

    // Parse the durations.
    let duration = parse_duration(&bench.duration).expect("invalid duration passed");
    let warmup_duration = parse_duration(&bench.warmup_duration).expect("invalid warmup duration passed");

    // Initialize connections.
    let connections = bench.connections;
    let confirmed_reads = cli.confirmed_reads;
    if !cli.quiet {
        println!("initializing {connections} connections with confirmed-reads={confirmed_reads}");
    }
    let (join_handles, conns): (Vec<_>, Vec<_>) = (0..connections)
        .map(|_| {
            let (jh, tx, rx) = handle.block_on(init_conn(&cli, &handle));
            (jh, (tx, rx))
        })
        .unzip();

    // Pre-compute transfer pairs.
    let transfer_pairs = &make_transfers(accounts, alpha);
    let transfers_per_worker = transfer_pairs.len() / conns.len();

    let warmup_start_all = Instant::now();
    let mut start_all = warmup_start_all;
    let barrier = &std::sync::Barrier::new(conns.len());
    let completed = Arc::new(AtomicU64::default());

    thread::scope(|scope| {
        if !cli.quiet {
            eprintln!("warming up for {}...", format_duration(warmup_duration));
        }
        let mut start_all = Some(&mut start_all);
        for (worker_idx, (mut rx, tx)) in conns.into_iter().enumerate() {
            let completed = completed.clone();
            let start_all = start_all.take();
            scope.spawn(move || {
                let mut run = || {
                    let mut transfers = 0;
                    let mut transfer_idx = worker_idx * transfers_per_worker;

                    while transfers < bench.max_inflight_reducers {
                        let (from, to) = match transfer_pairs.get(transfer_idx) {
                            Some(x) => *x,
                            None => {
                                transfer_idx = 0;
                                transfer_pairs[transfer_idx]
                            }
                        };
                        transfer_idx += 1;

                        let args = (from, to, amount);
                        let args = bsatn::to_vec(&args).unwrap().into();
                        tx.send(ClientMessage::CallReducer(CallReducer {
                            reducer: "transfer".into(),
                            args,
                            request_id: 0,
                            flags: CallReducerFlags::FullUpdate,
                        }))
                        .unwrap();
                        transfers += 1;
                    }

                    // Block until all confirmations arrived.
                    let mut recorded_transfers = 0;
                    while recorded_transfers < transfers {
                        match rx.blocking_recv() {
                            None => unreachable!(),
                            Some(ServerMessage::TransactionUpdate) => {}
                            Some(_) => continue,
                        }
                        recorded_transfers += 1;
                    }

                    transfers
                };

                while warmup_start_all.elapsed() < warmup_duration {
                    run();
                }

                if barrier.wait().is_leader() {
                    if !cli.quiet {
                        eprintln!("finished warmup...");
                        eprintln!("benchmarking for {}...", format_duration(duration));
                    }
                }
                let start = Instant::now();
                if let Some(start_all) = start_all {
                    *start_all = start;
                }

                while start.elapsed() < duration {
                    let transfers = run();
                    completed.fetch_add(transfers, Ordering::Relaxed);
                }
            });
        }
    });

    let completed = completed.load(Ordering::Relaxed);
    let elapsed = start_all.elapsed().as_secs_f64();
    let tps = completed as f64 / elapsed;

    if !cli.quiet {
        println!("ran for {elapsed} seconds");
        println!("completed {completed}");
        println!("throughput was {tps} TPS");
    }

    if let Some(path) = bench.tps_write_path.as_deref() {
        let path = Path::new(path);
        fs::write(path, format!("{tps}")).expect("Failed to write TPS to file {path}");
    }

    for handle in join_handles {
        handle.abort();
    }
}

#[derive(Parser)]
#[command(about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct Common {
    #[arg(short, long, default_value_t = false)]
    quiet: bool,

    #[arg(short, long, default_value = LOCALHOST)]
    server: String,

    #[arg(short, long, default_value = MODULE)]
    module: String,

    #[arg(long, default_value_t = CONFIRMED_READS)]
    confirmed_reads: bool,

    #[arg(long, default_value_t = ACCOUNTS)]
    accounts: u32,
}

#[derive(Subcommand)]
enum Commands {
    Seed(Seed),
    Bench(Bench),
}

#[derive(Args)]
struct Seed {
    #[command(flatten)]
    common: Common,

    #[arg(short, long, default_value_t = INIT_BALANCE)]
    initial_balance: i64,
}

#[derive(Args)]
struct Bench {
    #[command(flatten)]
    common: Common,

    #[arg(short, long, default_value_t = ALPHA)]
    alpha: f32,

    #[arg(long, default_value_t = AMOUNT)]
    amount: u32,

    #[arg(short, long, default_value_t = CONNECTIONS)]
    connections: usize,

    #[arg(long, default_value_t = MAX_INFLIGHT_REDUCERS)]
    max_inflight_reducers: u64,

    #[arg(short, long, default_value = DURATION)]
    duration: String,

    #[arg(short, long, default_value = WARMUP_DURATION)]
    warmup_duration: String,

    #[arg(short, long)]
    tps_write_path: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Seed(seed_args) => seed(&seed_args.common, seed_args),
        Commands::Bench(bench_args) => bench(&bench_args.common, bench_args),
    }
}
