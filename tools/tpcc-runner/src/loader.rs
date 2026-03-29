use anyhow::{Context, Result};
use rand::{rngs::StdRng, Rng, SeedableRng};
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
    let chunks = database_number_chunks(config.num_databases, config.load_parallelism);
    let mut handles = Vec::with_capacity(chunks.len());

    for (worker_idx, chunk) in chunks.into_iter().enumerate() {
        let config = config.clone();
        let topology = topology.clone();
        let thread_name = format!("tpcc-load-{worker_idx}");
        let handle = thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || -> Result<()> {
                for database_number in chunk {
                    run_one_database(&config, database_number, &topology)?;
                }
                Ok(())
            })
            .with_context(|| format!("failed to spawn {thread_name}"))?;
        handles.push(handle);
    }

    for handle in handles {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => anyhow::bail!("loader worker thread panicked"),
        }
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

macro_rules! time {
    ($span_name:literal { $($body:tt)*}) => {{
        #[allow(clippy::redundant_closure_call)]
        let before = std::time::Instant::now();
        log::info!("Span {} starting at {:?}", $span_name, before);
        let run = || -> anyhow::Result<_> { Ok({ $($body)* }) };
        let res = run();
        let elapsed = before.elapsed();
        log::info!("Span {} ended after {:?}", $span_name, elapsed);
        res?
    }}
}

fn run_one_database(config: &LoadConfig, database_number: u32, topology: &DatabaseTopology) -> Result<()> {
    time!("run_one_database" {
        let database_identity = topology.identity_for_database_number(database_number)?;
        log::info!(
            "starting tpcc load into {} / {} with {} warehouse(s)",
            config.connection.uri,
            database_identity,
            config.warehouses_per_database
        );

        let mut client = ModuleClient::connect(&config.connection, database_identity)?;
        client.subscribe_load_state()?;

        time!("reset" {
            if config.reset {
                client.reset_tpcc().context("failed to reset tpcc data")?;
            }
        });

        let request = time!("build_load_request" {
            build_load_request(config, database_number, topology)?
        });
        time!("configure_tpcc_load" {client
                                     .configure_tpcc_load(request)
                                     .context("failed to configure tpcc load")})?;

        time!("start_tpcc_load" {
            client.start_tpcc_load().context("failed to start tpcc load")?
        });

        time!("wait_for_load_completion" {
            wait_for_load_completion(&client, database_identity)?
        });

        time!("shutdown" {
            client.shutdown()
        });

        log::info!("tpcc load for database {database_identity} finished");
        Ok(())
    })
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
        batch_size: u32::try_from(config.batch_size).context("batch_size exceeds u32")?,
        seed: LOAD_SEED,
        load_c_last,
        base_ts: Timestamp::from(SystemTime::now()),
        spacetimedb_uri: config.connection.uri.clone(),
        database_identities,
    })
}

fn wait_for_load_completion(client: &ModuleClient, database_identity: spacetimedb_sdk::Identity) -> Result<()> {
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
                log::info!(
                    "tpcc load progress for {}: status={:?} phase={:?} chunks={} rows={} next=({},{},{},{})",
                    database_identity,
                    state.status,
                    state.phase,
                    state.chunks_completed,
                    state.rows_inserted,
                    state.next_warehouse_id,
                    state.next_district_id,
                    state.next_item_id,
                    state.next_order_id
                );
                last_logged = Some(current_progress);
            }

            match state.status {
                TpccLoadStatus::Complete => return Ok(()),
                TpccLoadStatus::Failed => {
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
