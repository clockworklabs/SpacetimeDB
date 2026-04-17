use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeSet;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::client::ModuleClient;
use crate::config::WaitConfig;
use crate::module_bindings::{TpccLoadState, TpccLoadStatus};
use crate::topology::lookup_database_identities;

enum StatusRequest {
    Check(DatabaseTarget),
    Shutdown,
}

#[derive(Clone, Copy)]
struct DatabaseTarget {
    database_number: u32,
    database_identity: spacetimedb_sdk::Identity,
}

struct StatusResponse {
    database_number: u32,
    database_identity: spacetimedb_sdk::Identity,
    state: Option<TpccLoadState>,
}

#[derive(Clone, Copy)]
enum ObservedStatus {
    Unknown,
    Missing,
    Idle,
    Running,
    Failed,
    Complete,
}

#[derive(Default)]
struct StatusCounts {
    unknown: usize,
    missing: usize,
    idle: usize,
    running: usize,
    failed: usize,
    complete: usize,
}

pub async fn run(config: WaitConfig) -> Result<()> {
    log::info!(
        "Waiting for tpcc load completion for {} databases on {} with {} worker(s)",
        config.num_databases,
        config.connection.uri,
        config.parallelism
    );

    let identities = lookup_database_identities(&config.connection, config.num_databases).await?;
    let targets: Vec<_> = (0..config.num_databases)
        .zip(identities.into_iter())
        .map(|(database_number, database_identity)| DatabaseTarget {
            database_number,
            database_identity,
        })
        .collect();

    let (request_tx, request_rx) = mpsc::channel::<StatusRequest>();
    let (response_tx, response_rx) = mpsc::channel::<Result<StatusResponse>>();
    let request_rx = Arc::new(Mutex::new(request_rx));
    let mut handles = Vec::with_capacity(config.parallelism);

    for worker_idx in 0..config.parallelism {
        let request_rx = Arc::clone(&request_rx);
        let response_tx = response_tx.clone();
        let connection = config.connection.clone();
        let database_prefix = config.connection.database_prefix.clone();
        let thread_name = format!("tpcc-wait-{worker_idx}");
        let handle = thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || worker_loop(&connection, &database_prefix, request_rx, response_tx))
            .with_context(|| format!("failed to spawn {thread_name}"))?;
        handles.push(handle);
    }
    drop(response_tx);

    let mut incomplete: BTreeSet<u32> = targets.iter().map(|target| target.database_number).collect();
    let mut observed = vec![ObservedStatus::Unknown; targets.len()];
    let progress = ProgressBar::new(u64::from(config.num_databases));
    progress.set_style(
        ProgressStyle::with_template("{spinner:.cyan} [{elapsed_precise}] {wide_bar} {pos}/{len} complete | {msg}")
            .expect("progress template should be valid")
            .tick_strings(&["-", "\\", "|", "/"]),
    );
    progress.enable_steady_tick(Duration::from_millis(120));
    let mut next_poll_at = Instant::now();
    update_progress(&progress, &observed, 0);

    while !incomplete.is_empty() {
        let now = Instant::now();
        if now < next_poll_at {
            thread::sleep(next_poll_at - now);
        }
        next_poll_at = Instant::now() + Duration::from_millis(config.poll_interval_ms);

        let mut round = 0usize;
        for database_number in incomplete.iter().copied() {
            request_tx
                .send(StatusRequest::Check(targets[database_number as usize]))
                .context("failed to send status request to worker")?;
            round += 1;
        }
        update_progress(&progress, &observed, round);

        let mut completed_this_round = Vec::new();
        let mut pending = round;
        for _ in 0..round {
            let response = response_rx
                .recv()
                .context("failed to receive status response from worker")??;
            let database_name = format!("{}-{}", config.connection.database_prefix, response.database_number);
            // progress.suspend(|| print_status_line(&database_name, response.database_identity, response.state.as_ref()));
            observed[response.database_number as usize] = observe_state(response.state.as_ref());
            pending -= 1;
            update_progress(&progress, &observed, pending);

            match response.state {
                Some(state) if state.status == TpccLoadStatus::Complete => {
                    completed_this_round.push(response.database_number);
                }
                Some(state) if state.status == TpccLoadStatus::Failed => {
                    progress.finish_and_clear();
                    bail!(
                        "{} failed: {:?}",
                        database_name,
                        state
                            .last_error
                            .unwrap_or_else(|| "load failed without an error message".to_string())
                    );
                }
                _ => {}
            }
        }

        for database_number in completed_this_round {
            incomplete.remove(&database_number);
        }

        if !incomplete.is_empty() {
            thread::sleep(Duration::from_millis(config.poll_interval_ms));
        }
    }

    for _ in 0..config.parallelism {
        request_tx
            .send(StatusRequest::Shutdown)
            .context("failed to send shutdown request to worker")?;
    }

    for handle in handles {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => bail!("wait worker thread panicked"),
        }
    }

    progress.finish_with_message("all databases complete");
    log::info!("tpcc load completed for all {} database(s)", config.num_databases);
    Ok(())
}

fn worker_loop(
    connection: &crate::config::ConnectionConfig,
    database_prefix: &str,
    request_rx: Arc<Mutex<mpsc::Receiver<StatusRequest>>>,
    response_tx: mpsc::Sender<Result<StatusResponse>>,
) -> Result<()> {
    loop {
        let request = {
            let rx = request_rx.lock().expect("request_rx mutex poisoned");
            rx.recv()
        };

        match request {
            Ok(StatusRequest::Check(target)) => {
                let response = query_status(connection, database_prefix, target);
                if response_tx.send(response).is_err() {
                    return Ok(());
                }
            }
            Ok(StatusRequest::Shutdown) => return Ok(()),
            Err(_) => return Ok(()),
        }
    }
}

fn query_status(
    connection: &crate::config::ConnectionConfig,
    database_prefix: &str,
    target: DatabaseTarget,
) -> Result<StatusResponse> {
    let database_name = format!("{}-{}", database_prefix, target.database_number);
    let mut client = ModuleClient::connect(connection, target.database_identity)
        .with_context(|| format!("failed to connect to {database_name}"))?;
    client
        .subscribe_load_state()
        .with_context(|| format!("failed to subscribe to load state for {database_name}"))?;
    let state = client.load_state();
    client.shutdown();

    Ok(StatusResponse {
        database_number: target.database_number,
        database_identity: target.database_identity,
        state,
    })
}

fn print_status_line(database_name: &str, database_identity: spacetimedb_sdk::Identity, state: Option<&TpccLoadState>) {
    if let Some(state) = state {
        println!(
            "{database_name} identity={database_identity} status={:?} phase={:?} chunks_completed={} rows_inserted={} next=({},{},{},{}) started_at={:?} updated_at={:?} completed_at={:?} last_error={:?}",
            state.status,
            state.phase,
            state.chunks_completed,
            state.rows_inserted,
            state.next_warehouse_id,
            state.next_district_id,
            state.next_item_id,
            state.next_order_id,
            state.started_at,
            state.updated_at,
            state.completed_at,
            state.last_error,
        );
    } else {
        println!("{database_name} identity={database_identity} load_state=missing");
    }
}

fn observe_state(state: Option<&TpccLoadState>) -> ObservedStatus {
    match state {
        None => ObservedStatus::Missing,
        Some(state) => match state.status {
            TpccLoadStatus::Idle => ObservedStatus::Idle,
            TpccLoadStatus::Running => ObservedStatus::Running,
            TpccLoadStatus::Failed => ObservedStatus::Failed,
            TpccLoadStatus::Complete => ObservedStatus::Complete,
        },
    }
}

fn count_states(observed: &[ObservedStatus]) -> StatusCounts {
    let mut counts = StatusCounts::default();
    for state in observed {
        match state {
            ObservedStatus::Unknown => counts.unknown += 1,
            ObservedStatus::Missing => counts.missing += 1,
            ObservedStatus::Idle => counts.idle += 1,
            ObservedStatus::Running => counts.running += 1,
            ObservedStatus::Failed => counts.failed += 1,
            ObservedStatus::Complete => counts.complete += 1,
        }
    }
    counts
}

fn update_progress(progress: &ProgressBar, observed: &[ObservedStatus], pending_checks: usize) {
    let counts = count_states(observed);
    progress.set_position(counts.complete as u64);
    progress.set_message(format!(
        "complete={} running={} idle={} missing={} failed={} unknown={} pending_checks={}",
        counts.complete, counts.running, counts.idle, counts.missing, counts.failed, counts.unknown, pending_checks,
    ));
}
