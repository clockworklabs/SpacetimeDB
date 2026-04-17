use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::client::ModuleClient;
use crate::config::LoadConfig;
use crate::module_bindings::{TpccLoadConfigRequest, TpccLoadStatus};
use crate::topology::DatabaseTopology;
use spacetimedb_sdk::Timestamp;

const LOAD_SEED: u64 = 0x5eed_5eed;

pub async fn run(config: LoadConfig) -> Result<()> {
    log::info!(
        "Loading tpcc dataset into {} databases on {} with parallelism {}",
        config.num_databases,
        config.connection.uri,
        config.load_parallelism
    );

    let topology = DatabaseTopology::for_load(&config).await?;
    let progress = Arc::new(LoadProgress::new());
    let chunks = database_number_chunks(config.num_databases, config.load_parallelism);
    let mut handles = Vec::with_capacity(chunks.len());

    for (worker_idx, chunk) in chunks.into_iter().enumerate() {
        let config = config.clone();
        let topology = topology.clone();
        let progress = Arc::clone(&progress);
        let thread_name = format!("tpcc-load-{worker_idx}");
        let handle = thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || -> Vec<DatabaseRunFailure> {
                let mut failures = Vec::new();
                for database_number in chunk {
                    let database_name = topology.database_name(database_number);
                    let progress_bar = progress.add_database(&database_name);
                    if let Err(error) = run_one_database(&config, database_number, &topology, &progress_bar) {
                        let database_name = topology.database_name(database_number);
                        let database_identity = topology.identity_for_database_number(database_number).ok();
                        progress_bar.abandon_with_message(format!("{database_name} failed: {error:#}"));
                        failures.push(DatabaseRunFailure {
                            database_number,
                            database_name,
                            database_identity,
                            error: format!("{error:#}"),
                        });
                    }
                }
                failures
            })
            .with_context(|| format!("failed to spawn {thread_name}"))?;
        handles.push(handle);
    }

    let mut failures = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok(worker_failures) => failures.extend(worker_failures),
            Err(_) => anyhow::bail!("loader worker thread panicked"),
        }
    }

    if !failures.is_empty() {
        for failure in &failures {
            log::error!(
                "tpcc load failed for {} (database {}): {}",
                failure.database_name,
                failure.database_number,
                failure.error
            );
        }
        anyhow::bail!(format_failure_report(&config, &failures));
    }

    log::info!("tpcc load finished");
    Ok(())
}

fn database_number_chunks(num_databases: u32, parallelism: usize) -> Vec<Vec<u32>> {
    let database_numbers: Vec<u32> = (0..num_databases).collect();
    let chunk_size = database_numbers.len().div_ceil(parallelism);
    database_numbers
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

struct DatabaseRunFailure {
    database_number: u32,
    database_name: String,
    database_identity: Option<spacetimedb_sdk::Identity>,
    error: String,
}

struct LoadProgress {
    multi: MultiProgress,
    style: ProgressStyle,
}

impl LoadProgress {
    fn new() -> Self {
        let style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("progress template should be valid")
            .tick_strings(&["-", "\\", "|", "/"]);
        Self {
            multi: MultiProgress::new(),
            style,
        }
    }

    fn add_database(&self, database_name: &str) -> ProgressBar {
        let progress = self.multi.add(ProgressBar::new_spinner());
        progress.set_style(self.style.clone());
        progress.enable_steady_tick(Duration::from_millis(120));
        progress.set_message(format!("{database_name}: connecting"));
        progress
    }
}

fn format_failure_report(config: &LoadConfig, failures: &[DatabaseRunFailure]) -> String {
    let mut message = String::from("tpcc load failed for one or more databases.");
    message.push_str("\nFailed databases:");

    for failure in failures {
        if let Some(identity) = failure.database_identity {
            message.push_str(&format!(
                "\n- {} (database_number={}, identity={}): {}",
                failure.database_name, failure.database_number, identity, failure.error
            ));
        } else {
            message.push_str(&format!(
                "\n- {} (database_number={}): {}",
                failure.database_name, failure.database_number, failure.error
            ));
        }
    }

    message.push_str("\nRetry only the failed databases with:");
    for failure in failures {
        message.push_str(&format!(
            "\ncargo run -p spacetimedb-cli -- call -s {} {} resume_tpcc_load",
            config.connection.uri, failure.database_name
        ));
    }

    message.push_str("\nIf you need to discard partial progress for a failed database and start that shard over:");
    for failure in failures {
        message.push_str(&format!(
            "\ncargo run -p spacetimedb-cli -- call -s {} {} restart_tpcc_load",
            config.connection.uri, failure.database_name
        ));
    }

    message
}

macro_rules! time {
    ($span_name:literal { $($body:tt)*}) => {{
        #[allow(clippy::redundant_closure_call)]
        let before = std::time::Instant::now();
        log::debug!("Span {} starting at {:?}", $span_name, before);
        let run = || -> anyhow::Result<_> { Ok({ $($body)* }) };
        let res = run();
        let elapsed = before.elapsed();
        log::debug!("Span {} ended after {:?}", $span_name, elapsed);
        res?
    }}
}

fn run_one_database(
    config: &LoadConfig,
    database_number: u32,
    topology: &DatabaseTopology,
    progress: &ProgressBar,
) -> Result<()> {
    time!("run_one_database" {
        let database_name = topology.database_name(database_number);
        let database_identity = topology.identity_for_database_number(database_number)?;
        progress.set_message(format!(
            "{database_name}: connecting to {} with {} warehouse(s)",
            database_identity, config.warehouses_per_database
        ));

        let mut client = ModuleClient::connect(&config.connection, database_identity)?;
        progress.set_message(format!("{database_name}: subscribing to load state"));
        client.subscribe_load_state()?;
        if has_load_state(&client) {
            progress.set_message(format!("{database_name}: existing load state detected"));
        } else {
            progress.set_message(format!("{database_name}: no existing load state"));
            let request = time!("build_load_request" {
                build_load_request(config, database_number, topology)?
            });
            progress.set_message(format!("{database_name}: configuring load"));
            time!("configure_tpcc_load" {client
                                         .configure_tpcc_load(request)
                                         .context("failed to configure tpcc load")})?;

            progress.set_message(format!("{database_name}: starting load"));
            time!("start_tpcc_load" {
                client.start_tpcc_load().context("failed to start tpcc load")?
            });

            // Maybe add a flag for whether to wait for completion or not.
            /*
            time!("wait_for_load_completion" {
                wait_for_load_completion(&client, &database_name, database_identity, progress)?
            });
            */
        }

        // fail_if_partial_load_detected(config, &database_name, &client)?;

        // time!("reset" {
        //     if config.reset {
        //         progress.set_message(format!("{database_name}: resetting existing data"));
        //         client.reset_tpcc().context("failed to reset tpcc data")?;
        //     }
        // });


        progress.set_message(format!("{database_name}: shutting down client"));
        time!("shutdown" {
            client.shutdown()
        });

        progress.finish_with_message(format!("{database_name}: complete"));
        log::info!("tpcc load for database {database_identity} finished");
       Ok(())
    })
}

fn has_load_state(client: &ModuleClient) -> bool {
    client.load_state().is_some()
}

fn fail_if_partial_load_detected(config: &LoadConfig, database_name: &str, client: &ModuleClient) -> Result<()> {
    let Some(state) = client.load_state() else {
        return Ok(());
    };

    if state.status == TpccLoadStatus::Complete {
        return Ok(());
    }

    anyhow::bail!(
        "detected existing partial tpcc load state for {database_name}: \
status={:?} phase={:?} chunks_completed={} rows_inserted={} next=({},{},{},{}) last_error={:?}\n\
Resume this shard with:\n\
cargo run -p spacetimedb-cli -- call -s {} {} resume_tpcc_load\n\
Or discard partial progress and restart just this shard with:\n\
cargo run -p spacetimedb-cli -- call -s {} {} restart_tpcc_load",
        state.status,
        state.phase,
        state.chunks_completed,
        state.rows_inserted,
        state.next_warehouse_id,
        state.next_district_id,
        state.next_item_id,
        state.next_order_id,
        state.last_error,
        config.connection.uri,
        database_name,
        config.connection.uri,
        database_name
    )
}

fn build_load_request(
    config: &LoadConfig,
    database_number: u32,
    topology: &DatabaseTopology,
) -> Result<TpccLoadConfigRequest> {
    let mut rng = StdRng::seed_from_u64(LOAD_SEED);
    let load_c_last = rng.random_range(0..=255);
    let mut database_identities = Vec::with_capacity(config.num_databases as usize);
    for database_number in 0..config.num_databases {
        database_identities.push(topology.identity_for_database_number(database_number)?);
    }

    Ok(TpccLoadConfigRequest {
        database_number,
        num_databases: config.num_databases,
        warehouses_per_database: config.warehouses_per_database,
        warehouse_id_offset: config.warehouse_id_offset,
        skip_items: config.skip_items,
        batch_size: u32::try_from(config.batch_size).context("batch_size exceeds u32")?,
        seed: LOAD_SEED,
        load_c_last,
        base_ts: Timestamp::from(SystemTime::now()),
        spacetimedb_uri: config.connection.uri.clone(),
        database_identities,
    })
}

fn wait_for_load_completion(
    client: &ModuleClient,
    database_name: &str,
    database_identity: spacetimedb_sdk::Identity,
    progress: &ProgressBar,
) -> Result<()> {
    let mut last_logged = None;

    loop {
        client.ensure_connected()?;

        if let Some(state) = client.load_state() {
            let current_progress = (
                state.status,
                state.phase,
                state.next_warehouse_id,
                state.next_district_id,
                state.next_item_id,
                state.next_order_id,
                state.chunks_completed,
                state.rows_inserted,
            );
            if last_logged != Some(current_progress) {
                progress.set_message(format!(
                    "{}: {:?} {:?} chunks={} rows={} next=({},{},{},{})",
                    database_name,
                    state.status,
                    state.phase,
                    state.chunks_completed,
                    state.rows_inserted,
                    state.next_warehouse_id,
                    state.next_district_id,
                    state.next_item_id,
                    state.next_order_id
                ));
                last_logged = Some(current_progress);
            }

            match state.status {
                TpccLoadStatus::Complete => return Ok(()),
                TpccLoadStatus::Failed => {
                    progress.abandon_with_message(format!("{database_name}: failed"));
                    anyhow::bail!(
                        "tpcc load failed for {}: {}",
                        database_identity,
                        state
                            .last_error
                            .unwrap_or_else(|| "load failed without an error message".to_string())
                    )
                }
                TpccLoadStatus::Idle | TpccLoadStatus::Running => {}
            }
        }

        thread::sleep(Duration::from_millis(250));
    }
}
