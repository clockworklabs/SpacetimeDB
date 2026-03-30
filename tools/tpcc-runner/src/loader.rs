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
<<<<<<< HEAD
        if batch.len() >= batch_size {
            client.queue_load_items(std::mem::take(&mut batch), pending, errors)?;
        }
    }
    if !batch.is_empty() {
        client.queue_load_items(batch, pending, errors)?;
    }
    Ok(())
||||||| 2c04a393f
        if batch.len() >= batch_size {
            client.queue_load_items(std::mem::take(&mut batch), &pending, &errors)?;
        }
    }
    if !batch.is_empty() {
        client.queue_load_items(batch, &pending, &errors)?;
    }
    Ok(())
=======

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
>>>>>>> jdetter/tpcc
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

<<<<<<< HEAD
    while !warehouse_batch.is_empty() {
        let split_at = warehouse_batch.len().min(batch_size);
        let remainder = warehouse_batch.split_off(split_at);
        let rows = std::mem::replace(&mut warehouse_batch, remainder);
        client.queue_load_remote_warehouses(rows, pending, errors)?;
    }

    Ok(())
||||||| 2c04a393f
    while !warehouse_batch.is_empty() {
        let split_at = warehouse_batch.len().min(batch_size);
        let remainder = warehouse_batch.split_off(split_at);
        let rows = std::mem::replace(&mut warehouse_batch, remainder);
        client.queue_load_remote_warehouses(rows, &pending, &errors)?;
    }

    Ok(())
=======
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
>>>>>>> jdetter/tpcc
}

fn wait_for_load_completion(client: &ModuleClient, database_identity: spacetimedb_sdk::Identity) -> Result<()> {
    let mut last_logged = None;

    loop {
        client.ensure_connected()?;

<<<<<<< HEAD
        for d_id in 1..=DISTRICTS_PER_WAREHOUSE {
            district_batch.push(District {
                district_key: pack_district_key(w_id, d_id),
                d_w_id: w_id,
                d_id,
                d_name: alpha_string(rng, 6, 10),
                d_street_1: alpha_numeric_string(rng, 10, 20),
                d_street_2: alpha_numeric_string(rng, 10, 20),
                d_city: alpha_string(rng, 10, 20),
                d_state: alpha_string(rng, 2, 2),
                d_zip: zip_code(rng),
                d_tax_bps: rng.random_range(0..=2_000),
                d_ytd_cents: DISTRICT_YTD_CENTS,
                d_next_o_id: CUSTOMERS_PER_DISTRICT + 1,
            });
        }
    }

    while !warehouse_batch.is_empty() {
        let split_at = warehouse_batch.len().min(batch_size);
        let remainder = warehouse_batch.split_off(split_at);
        let rows = std::mem::replace(&mut warehouse_batch, remainder);
        client.queue_load_warehouses(rows, pending, errors)?;
    }
    while !district_batch.is_empty() {
        let split_at = district_batch.len().min(batch_size);
        let remainder = district_batch.split_off(split_at);
        let rows = std::mem::replace(&mut district_batch, remainder);
        client.queue_load_districts(rows, pending, errors)?;
    }
    let _ = timestamp;
    Ok(())
}

fn load_stock(
    client: &ModuleClient,
    database_number: u16,
    warehouses_per_database: u16,
    batch_size: usize,
    rng: &mut StdRng,
    pending: &Arc<(Mutex<u64>, Condvar)>,
    errors: &Arc<Mutex<Option<anyhow::Error>>>,
) -> Result<()> {
    let mut batch = Vec::with_capacity(batch_size);
    for w_id in warehouses_range(database_number, warehouses_per_database) {
        for item_id in 1..=ITEMS {
            batch.push(Stock {
                stock_key: pack_stock_key(w_id, item_id),
                s_w_id: w_id,
                s_i_id: item_id,
                s_quantity: rng.random_range(10..=100),
                s_dist_01: alpha_string(rng, 24, 24),
                s_dist_02: alpha_string(rng, 24, 24),
                s_dist_03: alpha_string(rng, 24, 24),
                s_dist_04: alpha_string(rng, 24, 24),
                s_dist_05: alpha_string(rng, 24, 24),
                s_dist_06: alpha_string(rng, 24, 24),
                s_dist_07: alpha_string(rng, 24, 24),
                s_dist_08: alpha_string(rng, 24, 24),
                s_dist_09: alpha_string(rng, 24, 24),
                s_dist_10: alpha_string(rng, 24, 24),
                s_ytd: 0,
                s_order_cnt: 0,
                s_remote_cnt: 0,
                s_data: maybe_with_original(rng, 26, 50),
            });
            if batch.len() >= batch_size {
                client.queue_load_stocks(std::mem::take(&mut batch), pending, errors)?;
            }
        }
    }
    if !batch.is_empty() {
        client.queue_load_stocks(batch, pending, errors)?;
    }
    Ok(())
}

fn load_customers_history_orders(
    client: &ModuleClient,
    database_number: u16,
    warehouses_per_database: u16,
    batch_size: usize,
    timestamp: Timestamp,
    load_c_last: u32,
    rng: &mut StdRng,
    pending: &Arc<(Mutex<u64>, Condvar)>,
    errors: &Arc<Mutex<Option<anyhow::Error>>>,
) -> Result<()> {
    let mut customer_batch = Vec::with_capacity(batch_size);
    let mut history_batch = Vec::with_capacity(batch_size);
    let mut order_batch = Vec::with_capacity(batch_size);
    let mut new_order_batch = Vec::with_capacity(batch_size);
    let mut order_line_batch = Vec::with_capacity(batch_size);

    for w_id in warehouses_range(database_number, warehouses_per_database) {
        for d_id in 1..=DISTRICTS_PER_WAREHOUSE {
            let mut permutation: Vec<u32> = (1..=CUSTOMERS_PER_DISTRICT).collect();
            permutation.shuffle(rng);

            for c_id in 1..=CUSTOMERS_PER_DISTRICT {
                let credit = if rng.random_bool(0.10) { "BC" } else { "GC" };
                let last_name = if c_id <= 1_000 {
                    make_last_name(c_id - 1)
                } else {
                    make_last_name(nurand(rng, 255, 0, 999, load_c_last))
                };
                customer_batch.push(Customer {
                    customer_key: pack_customer_key(w_id, d_id, c_id),
                    c_w_id: w_id,
                    c_d_id: d_id,
                    c_id,
                    c_first: alpha_string(rng, 8, 16),
                    c_middle: "OE".to_string(),
                    c_last: last_name,
                    c_street_1: alpha_numeric_string(rng, 10, 20),
                    c_street_2: alpha_numeric_string(rng, 10, 20),
                    c_city: alpha_string(rng, 10, 20),
                    c_state: alpha_string(rng, 2, 2),
                    c_zip: zip_code(rng),
                    c_phone: numeric_string(rng, 16, 16),
                    c_since: timestamp,
                    c_credit: credit.to_string(),
                    c_credit_lim_cents: CUSTOMER_CREDIT_LIMIT_CENTS,
                    c_discount_bps: rng.random_range(0..=5_000),
                    c_balance_cents: CUSTOMER_INITIAL_BALANCE_CENTS,
                    c_ytd_payment_cents: CUSTOMER_INITIAL_YTD_PAYMENT_CENTS,
                    c_payment_cnt: 1,
                    c_delivery_cnt: 0,
                    c_data: alpha_numeric_string(rng, 300, 500),
                });
                history_batch.push(History {
                    history_id: 0,
                    h_c_id: c_id,
                    h_c_d_id: d_id,
                    h_c_w_id: w_id,
                    h_d_id: d_id,
                    h_w_id: w_id,
                    h_date: timestamp,
                    h_amount_cents: HISTORY_INITIAL_AMOUNT_CENTS,
                    h_data: alpha_numeric_string(rng, 12, 24),
                });

                if customer_batch.len() >= batch_size {
                    client.queue_load_customers(std::mem::take(&mut customer_batch), pending, errors)?;
                }
                if history_batch.len() >= batch_size {
                    client.queue_load_history(std::mem::take(&mut history_batch), pending, errors)?;
                }
||||||| 2c04a393f
        for d_id in 1..=DISTRICTS_PER_WAREHOUSE {
            district_batch.push(District {
                district_key: pack_district_key(w_id, d_id),
                d_w_id: w_id,
                d_id,
                d_name: alpha_string(rng, 6, 10),
                d_street_1: alpha_numeric_string(rng, 10, 20),
                d_street_2: alpha_numeric_string(rng, 10, 20),
                d_city: alpha_string(rng, 10, 20),
                d_state: alpha_string(rng, 2, 2),
                d_zip: zip_code(rng),
                d_tax_bps: rng.random_range(0..=2_000),
                d_ytd_cents: DISTRICT_YTD_CENTS,
                d_next_o_id: CUSTOMERS_PER_DISTRICT + 1,
            });
        }
    }

    while !warehouse_batch.is_empty() {
        let split_at = warehouse_batch.len().min(batch_size);
        let remainder = warehouse_batch.split_off(split_at);
        let rows = std::mem::replace(&mut warehouse_batch, remainder);
        client.queue_load_warehouses(rows, &pending, &errors)?;
    }
    while !district_batch.is_empty() {
        let split_at = district_batch.len().min(batch_size);
        let remainder = district_batch.split_off(split_at);
        let rows = std::mem::replace(&mut district_batch, remainder);
        client.queue_load_districts(rows, &pending, &errors)?;
    }
    let _ = timestamp;
    Ok(())
}

fn load_stock(
    client: &ModuleClient,
    database_number: u16,
    warehouses_per_database: u16,
    batch_size: usize,
    rng: &mut StdRng,
    pending: &Arc<(Mutex<u64>, Condvar)>,
    errors: &Arc<Mutex<Option<anyhow::Error>>>,
) -> Result<()> {
    let mut batch = Vec::with_capacity(batch_size);
    for w_id in warehouses_range(database_number, warehouses_per_database) {
        for item_id in 1..=ITEMS {
            batch.push(Stock {
                stock_key: pack_stock_key(w_id, item_id),
                s_w_id: w_id,
                s_i_id: item_id,
                s_quantity: rng.random_range(10..=100),
                s_dist_01: alpha_string(rng, 24, 24),
                s_dist_02: alpha_string(rng, 24, 24),
                s_dist_03: alpha_string(rng, 24, 24),
                s_dist_04: alpha_string(rng, 24, 24),
                s_dist_05: alpha_string(rng, 24, 24),
                s_dist_06: alpha_string(rng, 24, 24),
                s_dist_07: alpha_string(rng, 24, 24),
                s_dist_08: alpha_string(rng, 24, 24),
                s_dist_09: alpha_string(rng, 24, 24),
                s_dist_10: alpha_string(rng, 24, 24),
                s_ytd: 0,
                s_order_cnt: 0,
                s_remote_cnt: 0,
                s_data: maybe_with_original(rng, 26, 50),
            });
            if batch.len() >= batch_size {
                client.queue_load_stocks(std::mem::take(&mut batch), &pending, &errors)?;
            }
        }
    }
    if !batch.is_empty() {
        client.queue_load_stocks(batch, &pending, &errors)?;
    }
    Ok(())
}

fn load_customers_history_orders(
    client: &ModuleClient,
    database_number: u16,
    warehouses_per_database: u16,
    batch_size: usize,
    timestamp: Timestamp,
    load_c_last: u32,
    rng: &mut StdRng,
    pending: &Arc<(Mutex<u64>, Condvar)>,
    errors: &Arc<Mutex<Option<anyhow::Error>>>,
) -> Result<()> {
    let mut customer_batch = Vec::with_capacity(batch_size);
    let mut history_batch = Vec::with_capacity(batch_size);
    let mut order_batch = Vec::with_capacity(batch_size);
    let mut new_order_batch = Vec::with_capacity(batch_size);
    let mut order_line_batch = Vec::with_capacity(batch_size);

    for w_id in warehouses_range(database_number, warehouses_per_database) {
        for d_id in 1..=DISTRICTS_PER_WAREHOUSE {
            let mut permutation: Vec<u32> = (1..=CUSTOMERS_PER_DISTRICT).collect();
            permutation.shuffle(rng);

            for c_id in 1..=CUSTOMERS_PER_DISTRICT {
                let credit = if rng.random_bool(0.10) { "BC" } else { "GC" };
                let last_name = if c_id <= 1_000 {
                    make_last_name(c_id - 1)
                } else {
                    make_last_name(nurand(rng, 255, 0, 999, load_c_last))
                };
                customer_batch.push(Customer {
                    customer_key: pack_customer_key(w_id, d_id, c_id),
                    c_w_id: w_id,
                    c_d_id: d_id,
                    c_id,
                    c_first: alpha_string(rng, 8, 16),
                    c_middle: "OE".to_string(),
                    c_last: last_name,
                    c_street_1: alpha_numeric_string(rng, 10, 20),
                    c_street_2: alpha_numeric_string(rng, 10, 20),
                    c_city: alpha_string(rng, 10, 20),
                    c_state: alpha_string(rng, 2, 2),
                    c_zip: zip_code(rng),
                    c_phone: numeric_string(rng, 16, 16),
                    c_since: timestamp,
                    c_credit: credit.to_string(),
                    c_credit_lim_cents: CUSTOMER_CREDIT_LIMIT_CENTS,
                    c_discount_bps: rng.random_range(0..=5_000),
                    c_balance_cents: CUSTOMER_INITIAL_BALANCE_CENTS,
                    c_ytd_payment_cents: CUSTOMER_INITIAL_YTD_PAYMENT_CENTS,
                    c_payment_cnt: 1,
                    c_delivery_cnt: 0,
                    c_data: alpha_numeric_string(rng, 300, 500),
                });
                history_batch.push(History {
                    history_id: 0,
                    h_c_id: c_id,
                    h_c_d_id: d_id,
                    h_c_w_id: w_id,
                    h_d_id: d_id,
                    h_w_id: w_id,
                    h_date: timestamp,
                    h_amount_cents: HISTORY_INITIAL_AMOUNT_CENTS,
                    h_data: alpha_numeric_string(rng, 12, 24),
                });

                if customer_batch.len() >= batch_size {
                    client.queue_load_customers(std::mem::take(&mut customer_batch), &pending, &errors)?;
                }
                if history_batch.len() >= batch_size {
                    client.queue_load_history(std::mem::take(&mut history_batch), &pending, &errors)?;
                }
=======
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
>>>>>>> jdetter/tpcc
            }

<<<<<<< HEAD
            for o_id in 1..=CUSTOMERS_PER_DISTRICT {
                let customer_id = permutation[(o_id - 1) as usize];
                let delivered = o_id < NEW_ORDER_START;
                let order_line_count = rng.random_range(5..=15) as u8;
                order_batch.push(OOrder {
                    order_key: pack_order_key(w_id, d_id, o_id),
                    o_w_id: w_id,
                    o_d_id: d_id,
                    o_id,
                    o_c_id: customer_id,
                    o_entry_d: timestamp,
                    o_carrier_id: if delivered {
                        Some(rng.random_range(1..=10))
                    } else {
                        None
                    },
                    o_ol_cnt: order_line_count,
                    o_all_local: true,
                });
                if !delivered {
                    new_order_batch.push(NewOrder {
                        new_order_key: pack_order_key(w_id, d_id, o_id),
                        no_w_id: w_id,
                        no_d_id: d_id,
                        no_o_id: o_id,
                    });
                }

                for ol_number in 1..=order_line_count {
                    order_line_batch.push(OrderLine {
                        order_line_key: pack_order_line_key(w_id, d_id, o_id, ol_number),
                        ol_w_id: w_id,
                        ol_d_id: d_id,
                        ol_o_id: o_id,
                        ol_number,
                        ol_i_id: rng.random_range(1..=ITEMS),
                        ol_supply_w_id: w_id,
                        ol_delivery_d: if delivered { Some(timestamp) } else { None },
                        ol_quantity: 5,
                        ol_amount_cents: if delivered { 0 } else { rng.random_range(1..=999_999) },
                        ol_dist_info: alpha_string(rng, 24, 24),
                    });
                    if order_line_batch.len() >= batch_size {
                        client.queue_load_order_lines(std::mem::take(&mut order_line_batch), pending, errors)?;
                    }
                }

                if order_batch.len() >= batch_size {
                    client.queue_load_orders(std::mem::take(&mut order_batch), pending, errors)?;
                }
                if new_order_batch.len() >= batch_size {
                    client.queue_load_new_orders(std::mem::take(&mut new_order_batch), pending, errors)?;
||||||| 2c04a393f
            for o_id in 1..=CUSTOMERS_PER_DISTRICT {
                let customer_id = permutation[(o_id - 1) as usize];
                let delivered = o_id < NEW_ORDER_START;
                let order_line_count = rng.random_range(5..=15) as u8;
                order_batch.push(OOrder {
                    order_key: pack_order_key(w_id, d_id, o_id),
                    o_w_id: w_id,
                    o_d_id: d_id,
                    o_id,
                    o_c_id: customer_id,
                    o_entry_d: timestamp,
                    o_carrier_id: if delivered {
                        Some(rng.random_range(1..=10))
                    } else {
                        None
                    },
                    o_ol_cnt: order_line_count,
                    o_all_local: true,
                });
                if !delivered {
                    new_order_batch.push(NewOrder {
                        new_order_key: pack_order_key(w_id, d_id, o_id),
                        no_w_id: w_id,
                        no_d_id: d_id,
                        no_o_id: o_id,
                    });
                }

                for ol_number in 1..=order_line_count {
                    order_line_batch.push(OrderLine {
                        order_line_key: pack_order_line_key(w_id, d_id, o_id, ol_number),
                        ol_w_id: w_id,
                        ol_d_id: d_id,
                        ol_o_id: o_id,
                        ol_number,
                        ol_i_id: rng.random_range(1..=ITEMS),
                        ol_supply_w_id: w_id,
                        ol_delivery_d: if delivered { Some(timestamp) } else { None },
                        ol_quantity: 5,
                        ol_amount_cents: if delivered { 0 } else { rng.random_range(1..=999_999) },
                        ol_dist_info: alpha_string(rng, 24, 24),
                    });
                    if order_line_batch.len() >= batch_size {
                        client.queue_load_order_lines(std::mem::take(&mut order_line_batch), &pending, &errors)?;
                    }
                }

                if order_batch.len() >= batch_size {
                    client.queue_load_orders(std::mem::take(&mut order_batch), &pending, &errors)?;
                }
                if new_order_batch.len() >= batch_size {
                    client.queue_load_new_orders(std::mem::take(&mut new_order_batch), &pending, &errors)?;
=======
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
>>>>>>> jdetter/tpcc
                }
                TpccLoadStatus::Idle | TpccLoadStatus::Running => {}
            }
        }

<<<<<<< HEAD
    if !customer_batch.is_empty() {
        client.queue_load_customers(customer_batch, pending, errors)?;
    }
    if !history_batch.is_empty() {
        client.queue_load_history(history_batch, pending, errors)?;
    }
    if !order_batch.is_empty() {
        client.queue_load_orders(order_batch, pending, errors)?;
    }
    if !new_order_batch.is_empty() {
        client.queue_load_new_orders(new_order_batch, pending, errors)?;
    }
    if !order_line_batch.is_empty() {
        client.queue_load_order_lines(order_line_batch, pending, errors)?;
    }

    Ok(())
}

fn wait_for_pending(pending: &Arc<(Mutex<u64>, Condvar)>) {
    let (lock, cvar) = pending.as_ref();
    let mut guard = lock.lock().unwrap();
    while *guard > 0 {
        guard = cvar.wait(guard).unwrap();
    }
}

fn take_first_error(errors: &Arc<Mutex<Option<anyhow::Error>>>) -> Result<()> {
    let mut guard = errors.lock().unwrap();
    if let Some(err) = guard.take() {
        Err(err)
    } else {
        Ok(())
||||||| 2c04a393f
    if !customer_batch.is_empty() {
        client.queue_load_customers(customer_batch, &pending, &errors)?;
    }
    if !history_batch.is_empty() {
        client.queue_load_history(history_batch, &pending, &errors)?;
    }
    if !order_batch.is_empty() {
        client.queue_load_orders(order_batch, &pending, &errors)?;
    }
    if !new_order_batch.is_empty() {
        client.queue_load_new_orders(new_order_batch, &pending, &errors)?;
    }
    if !order_line_batch.is_empty() {
        client.queue_load_order_lines(order_line_batch, &pending, &errors)?;
    }

    Ok(())
}

fn wait_for_pending(pending: &Arc<(Mutex<u64>, Condvar)>) {
    let (lock, cvar) = pending.as_ref();
    let mut guard = lock.lock().unwrap();
    while *guard > 0 {
        guard = cvar.wait(guard).unwrap();
    }
}

fn take_first_error(errors: &Arc<Mutex<Option<anyhow::Error>>>) -> Result<()> {
    let mut guard = errors.lock().unwrap();
    if let Some(err) = guard.take() {
        Err(err)
    } else {
        Ok(())
=======
        thread::sleep(Duration::from_millis(250));
>>>>>>> jdetter/tpcc
    }
}
