//! Rust client for Postgres transfer benchmark.
//!
//! The Postgres equivalent of SpacetimeDB's Rust client:
//! - Direct binary protocol (no HTTP, no JSON, no Node.js)
//! - Multi-threaded Tokio runtime
//! - Batched queries with prepared statements
//! - Stored procedure (do_transfer) — single round-trip per transfer

use clap::{Args, Parser, Subcommand};
use humantime::{format_duration, parse_duration};
use rand::{distributions::Distribution, SeedableRng};
use rand::rngs::StdRng;
use rand_distr::Zipf;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::{fs, thread};
use tokio::runtime::{self, Runtime};
use tokio_postgres::{Client, NoTls, Statement};

const PG_URL: &str = "postgres://postgres:postgres@127.0.0.1:5432/postgres";
const DURATION: &str = "10s";
const WARMUP_DURATION: &str = "5s";
const ALPHA: f64 = 0.5;
const CONNECTIONS: usize = 50;
const INIT_BALANCE: i64 = 10_000_000;
const ACCOUNTS: u32 = 100_000;
const BATCH_SIZE: u64 = 256;

fn enter_or_create_runtime(threads: usize) -> (Option<Runtime>, runtime::Handle) {
    match runtime::Handle::try_current() {
        Err(e) if e.is_missing_context() => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(threads)
                .thread_name("pg-bench")
                .build()
                .unwrap();
            let handle = rt.handle().clone();
            (Some(rt), handle)
        }
        Ok(handle) => (None, handle),
        Err(_) => unimplemented!(),
    }
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

fn make_transfers(accounts: u32, alpha: f64) -> Vec<(i32, i32)> {
    let dist = Zipf::new(accounts as u64, alpha).unwrap();
    let mut rng = StdRng::seed_from_u64(0x12345678);
    (0..10_000_000)
        .filter_map(|_| {
            let (from, to) = pick_two_distinct(|| dist.sample(&mut rng) as u32, 32);
            if from >= accounts || to >= accounts || from == to {
                None
            } else {
                Some((from as i32, to as i32))
            }
        })
        .collect()
}

async fn connect(pg_url: &str) -> (Client, tokio::task::JoinHandle<()>) {
    let (client, connection) = tokio_postgres::connect(pg_url, NoTls)
        .await
        .expect("Failed to connect to Postgres");
    let jh = tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Postgres connection error: {}", e);
        }
    });
    (client, jh)
}

async fn ensure_stored_procedure(client: &Client) {
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION do_transfer(
                p_from_id INTEGER, p_to_id INTEGER, p_amount BIGINT
            ) RETURNS VOID AS $$
            DECLARE v_from BIGINT; v_to BIGINT;
            BEGIN
                IF p_from_id = p_to_id OR p_amount <= 0 THEN RETURN; END IF;
                IF p_from_id < p_to_id THEN
                    SELECT balance INTO v_from FROM accounts WHERE id = p_from_id FOR UPDATE;
                    SELECT balance INTO v_to   FROM accounts WHERE id = p_to_id   FOR UPDATE;
                ELSE
                    SELECT balance INTO v_to   FROM accounts WHERE id = p_to_id   FOR UPDATE;
                    SELECT balance INTO v_from FROM accounts WHERE id = p_from_id FOR UPDATE;
                END IF;
                IF v_from IS NULL OR v_to IS NULL THEN RAISE EXCEPTION 'account_missing'; END IF;
                IF v_from < p_amount THEN RETURN; END IF;
                UPDATE accounts SET balance = balance - p_amount WHERE id = p_from_id;
                UPDATE accounts SET balance = balance + p_amount WHERE id = p_to_id;
            END; $$ LANGUAGE plpgsql;",
        )
        .await
        .expect("Failed to create stored procedure");
}

fn seed(cli: &Common, seed_args: &Seed) {
    let (_runtime, handle) = enter_or_create_runtime(1);
    handle.block_on(async {
        let (client, _jh) = connect(&cli.pg_url).await;
        client.batch_execute(
            "CREATE TABLE IF NOT EXISTS accounts (id INTEGER PRIMARY KEY, balance BIGINT NOT NULL)",
        ).await.unwrap();
        client.execute("DELETE FROM accounts", &[]).await.unwrap();

        let batch: u32 = 10_000;
        let mut start: u32 = 0;
        while start < cli.accounts {
            let end = std::cmp::min(start + batch, cli.accounts);
            let mut q = String::from("INSERT INTO accounts (id, balance) VALUES ");
            for id in start..end {
                if id > start { q.push(','); }
                q.push_str(&format!("({}, {})", id, seed_args.initial_balance));
            }
            client.batch_execute(&q).await.unwrap();
            start = end;
        }
        ensure_stored_procedure(&client).await;
        if !cli.quiet {
            println!("seeded {} accounts with balance {}", cli.accounts, seed_args.initial_balance);
        }
    });
}

fn bench(cli: &Common, b: &Bench) {
    let (_runtime, handle) = enter_or_create_runtime(b.connections);

    if !cli.quiet {
        println!("Benchmark parameters:");
        println!("alpha={}, accounts={}, batch_size={}", b.alpha, cli.accounts, b.batch_size);
        println!();
    }

    let duration = parse_duration(&b.duration).expect("invalid duration");
    let warmup_duration = parse_duration(&b.warmup_duration).expect("invalid warmup");
    let connections = b.connections;

    if !cli.quiet {
        println!("initializing {connections} Postgres connections...");
    }

    // Create connections + prepare statements
    let conns: Vec<(Arc<Client>, Statement, tokio::task::JoinHandle<()>)> = (0..connections)
        .map(|_| {
            handle.block_on(async {
                let (client, jh) = connect(&cli.pg_url).await;
                let stmt = client
                    .prepare("SELECT do_transfer($1, $2, $3)")
                    .await
                    .expect("Failed to prepare");
                (Arc::new(client), stmt, jh)
            })
        })
        .collect();

    let transfer_pairs = Arc::new(make_transfers(cli.accounts, b.alpha));
    let transfers_per_worker = transfer_pairs.len() / conns.len();
    let batch_size = b.batch_size;

    let warmup_start = Instant::now();
    let mut bench_start = warmup_start;
    let barrier = &std::sync::Barrier::new(conns.len());
    let completed = Arc::new(AtomicU64::default());

    thread::scope(|scope| {
        if !cli.quiet {
            eprintln!("warming up for {}...", format_duration(warmup_duration));
        }

        let mut start_ref = Some(&mut bench_start);

        for (worker_idx, (client, stmt, _jh)) in conns.iter().enumerate() {
            let completed = completed.clone();
            let start_ref = start_ref.take();
            let pairs = transfer_pairs.clone();
            let client = client.clone();
            let stmt = stmt.clone();

            scope.spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                let run = || -> u64 {
                    let base_idx = worker_idx * transfers_per_worker;

                    // Fire a batch of concurrent queries
                    rt.block_on(async {
                        let mut handles = Vec::with_capacity(batch_size as usize);

                        for i in 0..batch_size {
                            let idx = (base_idx + i as usize) % pairs.len();
                            let (from_id, to_id) = pairs[idx];
                            let c = client.clone();
                            let s = stmt.clone();

                            handles.push(tokio::spawn(async move {
                                let amount: i64 = 1;
                                let _ = c.execute(&s, &[&from_id, &to_id, &amount]).await;
                            }));
                        }

                        let mut count = 0u64;
                        for h in handles {
                            if h.await.is_ok() {
                                count += 1;
                            }
                        }
                        count
                    })
                };

                // Warmup
                while warmup_start.elapsed() < warmup_duration {
                    run();
                }

                if barrier.wait().is_leader() && !cli.quiet {
                    eprintln!("finished warmup...");
                    eprintln!("benchmarking for {}...", format_duration(duration));
                }

                let local_start = Instant::now();
                if let Some(s) = start_ref {
                    *s = local_start;
                }

                // Benchmark
                while local_start.elapsed() < duration {
                    let n = run();
                    completed.fetch_add(n, Ordering::Relaxed);
                }
            });
        }
    });

    let completed = completed.load(Ordering::Relaxed);
    let elapsed = bench_start.elapsed().as_secs_f64();
    let tps = completed as f64 / elapsed;

    if !cli.quiet {
        println!("ran for {elapsed} seconds");
        println!("completed {completed}");
        println!("throughput was {tps} TPS");
    }

    if let Some(path) = b.tps_write_path.as_deref() {
        fs::write(Path::new(path), format!("{tps}")).expect("Failed to write TPS");
    }
}

#[derive(Parser)]
#[command(about = "Postgres Rust transfer benchmark")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct Common {
    #[arg(short, long, default_value_t = false)]
    quiet: bool,
    #[arg(long, default_value = PG_URL)]
    pg_url: String,
    #[arg(long, default_value_t = ACCOUNTS)]
    accounts: u32,
}

#[derive(Subcommand)]
enum Commands { Seed(Seed), Bench(Bench) }

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
    alpha: f64,
    #[arg(short, long, default_value_t = CONNECTIONS)]
    connections: usize,
    #[arg(long, default_value_t = BATCH_SIZE)]
    batch_size: u64,
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
        Commands::Seed(s) => seed(&s.common, s),
        Commands::Bench(b) => bench(&b.common, b),
    }
}
