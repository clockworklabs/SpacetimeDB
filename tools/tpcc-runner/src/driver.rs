use anyhow::{anyhow, bail, Context, Result};
use rand::{rngs::StdRng, Rng, SeedableRng};
use spacetimedb_sdk::DbContext;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;

use crate::client::{expect_ok, ModuleClient};
use crate::config::{default_run_id, DriverConfig};
use crate::metrics_module_bindings::{record_txn_bucket_count, DbConnection as MetricsDbConnection};
use crate::metrics_module_client::connect_metrics_module_async;
use crate::module_bindings::*;
use crate::protocol::{
    RegisterDriverRequest, RegisterDriverResponse, RunSchedule, ScheduleResponse, SubmitSummaryRequest,
};
use crate::summary::{
    log_driver_summary, write_json, DriverSummary, DriverSummaryMeta, SharedMetrics, TransactionKind, TransactionRecord,
};
use crate::topology::DatabaseTopology;
use crate::tpcc::*;

const STARTUP_STAGGER_WINDOW_MS: u64 = 18_000;
const METRICS_FLUSH_INTERVAL_MS: u64 = 500;
type MetricsClientPool = Vec<Arc<MetricsDbConnection>>;
type DatabaseClientPool = Vec<Arc<ModuleClient>>;
type SharedDatabaseClients = BTreeMap<u32, DatabaseClientPool>;

struct TerminalRuntime {
    config: DriverConfig,
    client: Arc<ModuleClient>,
    metrics: SharedMetrics,
    txn_bucket_reporter: Arc<TxnBucketReporter>,
    abort: Arc<AtomicBool>,
    start_logged: Arc<AtomicBool>,
    request_ids: Arc<AtomicU64>,
    schedule: RunSchedule,
    run_constants: RunConstants,
    assignment: TerminalAssignment,
    database_identity: spacetimedb_sdk::Identity,
    seed: u64,
}

struct TxnBucketReporter {
    run_start_ms: u64,
    pending: Mutex<BTreeMap<u64, u64>>,
    metrics_clients: MetricsClientPool,
    next_client: AtomicU64,
}

struct TransactionContext<'a> {
    client: &'a ModuleClient,
    config: &'a DriverConfig,
    run_id: &'a str,
    driver_id: &'a str,
    assignment: &'a TerminalAssignment,
    constants: &'a RunConstants,
    request_ids: &'a AtomicU64,
}

pub async fn run(config: DriverConfig) -> Result<()> {
    let (config, schedule) = resolve_driver_setup(config).await?;
    let run_id = schedule.run_id.clone();
    let output_dir = resolve_output_dir(&config, &run_id);
    log::info!(
        "driver {} resolved setup for run {}; creating output dir {}",
        config.driver_id,
        run_id,
        output_dir.display()
    );
    fs::create_dir_all(&output_dir).with_context(|| format!("failed to create {}", output_dir.display()))?;
    let topology = DatabaseTopology::for_driver(&config).await?;
    let used_database_numbers = databases_for_warehouse_slice(&config);
    let database_summary = describe_databases(&topology, &used_database_numbers);
    log::info!(
        "driver {} topology ready; warehouse slice {}..={} uses databases {:?}",
        config.driver_id,
        config.warehouse_start,
        config.warehouse_end(),
        used_database_numbers
    );

    let events_path = output_dir.join("txn_events.ndjson");
    let summary_path = output_dir.join("summary.json");
    let metrics = SharedMetrics::create(&run_id, &config.driver_id, &events_path)?;

    let run_constants = {
        let mut rng = StdRng::seed_from_u64(schedule.measure_start_ms ^ u64::from(config.warehouse_start));
        generate_run_constants(&mut rng)
    };

    let abort = Arc::new(AtomicBool::new(false));
    let start_logged = Arc::new(AtomicBool::new(false));
    let request_ids = Arc::new(AtomicU64::new(1));
    let mut tasks = JoinSet::new();
    log::info!(
        "driver {} connecting {} metrics database client(s)",
        config.driver_id,
        config.connections_per_database
    );
    let metrics_clients = connect_metrics_clients(&config).await?;
    log::info!(
        "driver {} connected {} metrics database client(s)",
        config.driver_id,
        metrics_clients.len()
    );
    let txn_bucket_reporter = Arc::new(TxnBucketReporter::new(
        schedule.warmup_start_ms,
        metrics_clients.clone(),
    ));
    let txn_bucket_reporter_shutdown = Arc::new(AtomicBool::new(false));
    let txn_bucket_reporter_task =
        spawn_txn_bucket_reporter(txn_bucket_reporter.clone(), txn_bucket_reporter_shutdown.clone());
    log::info!(
        "driver {} connecting {} shared database client(s) across {} database(s) with pool size {}",
        config.driver_id,
        used_database_numbers.len() * config.connections_per_database,
        used_database_numbers.len(),
        config.connections_per_database
    );
    let shared_database_clients = connect_shared_database_clients(&config, &topology, &used_database_numbers).await?;
    log::info!(
        "driver {} connected {} shared database client(s) across {} database(s)",
        config.driver_id,
        total_shared_database_clients(&shared_database_clients),
        shared_database_clients.len()
    );

    log::info!(
        "driver {} ready for run {}: warehouses {}..={} terminals={} warmup_start_ms={} measure_start_ms={} measure_end_ms={}",
        config.driver_id,
        run_id,
        config.warehouse_start,
        config.warehouse_end(),
        config.terminals(),
        schedule.warmup_start_ms,
        schedule.measure_start_ms,
        schedule.measure_end_ms
    );
    log::info!(
        "driver {} shared metrics connections ready; launching {} terminal task(s) across {} database(s) with {} shared database connection(s)",
        config.driver_id,
        config.terminals(),
        shared_database_clients.len(),
        total_shared_database_clients(&shared_database_clients)
    );

    for warehouse_id in config.warehouse_start..=config.warehouse_end() {
        let database_number = topology.database_number_for_warehouse(warehouse_id)?;
        let database_identity = topology.identity_for_warehouse(warehouse_id)?;
        for district_id in 1..=DISTRICTS_PER_WAREHOUSE {
            let assignment = TerminalAssignment {
                terminal_id: terminal_id(warehouse_id, district_id),
                warehouse_id,
                district_id,
            };
            let client =
                select_pooled_database_client(&shared_database_clients, database_number, assignment.terminal_id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow!(
                            "missing shared database client pool for {}",
                            topology.database_name(database_number)
                        )
                    })?;
            let terminal_seed = schedule.measure_start_ms ^ ((assignment.terminal_id as u64) << 32) ^ 0xabcdu64;
            let terminal_config = config.clone();
            let terminal_metrics = metrics.clone();
            let terminal_abort = abort.clone();
            let terminal_start_logged = start_logged.clone();
            let terminal_constants = run_constants.clone();
            let terminal_schedule = schedule.clone();
            let terminal_request_ids = request_ids.clone();
            let runtime = TerminalRuntime {
                config: terminal_config,
                client: client.clone(),
                metrics: terminal_metrics,
                txn_bucket_reporter: txn_bucket_reporter.clone(),
                abort: terminal_abort,
                start_logged: terminal_start_logged,
                request_ids: terminal_request_ids,
                schedule: terminal_schedule,
                run_constants: terminal_constants,
                assignment,
                database_identity,
                seed: terminal_seed,
            };
            tasks.spawn(run_terminal(runtime));
        }
    }

    let mut first_error: Option<anyhow::Error> = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                abort.store(true, Ordering::Relaxed);
                if first_error.is_none() {
                    log::error!(
                        "driver {} aborting run {} after terminal task error: {err:#}",
                        config.driver_id,
                        run_id
                    );
                    first_error = Some(err);
                    tasks.abort_all();
                }
            }
            Err(err) => {
                abort.store(true, Ordering::Relaxed);
                if first_error.is_none() {
                    let err = anyhow!("terminal task failed: {}", err);
                    log::error!(
                        "driver {} aborting run {} after terminal task failure: {err:#}",
                        config.driver_id,
                        run_id
                    );
                    first_error = Some(err);
                    tasks.abort_all();
                }
            }
        }
    }
    if let Some(err) = first_error {
        if let Err(flush_err) = stop_txn_bucket_reporter(
            txn_bucket_reporter.as_ref(),
            txn_bucket_reporter_shutdown,
            txn_bucket_reporter_task,
        )
        .await
        {
            log::error!(
                "driver {} failed to stop txn bucket reporter after terminal error: {flush_err:#}",
                config.driver_id
            );
        }
        drop(txn_bucket_reporter);
        shutdown_shared_database_clients(shared_database_clients).await;
        shutdown_metrics_clients(metrics_clients).await;
        return Err(err);
    }

    stop_txn_bucket_reporter(
        txn_bucket_reporter.as_ref(),
        txn_bucket_reporter_shutdown,
        txn_bucket_reporter_task,
    )
    .await?;
    drop(txn_bucket_reporter);
    harvest_delivery_completions(&config, &schedule, &metrics, &shared_database_clients).await?;
    shutdown_shared_database_clients(shared_database_clients).await;
    shutdown_metrics_clients(metrics_clients).await;

    let summary = metrics.finalize(DriverSummaryMeta {
        run_id: run_id.clone(),
        driver_id: config.driver_id.clone(),
        uri: config.connection.uri.clone(),
        database: database_summary,
        terminal_start: config.terminal_start(),
        terminals: config.terminals(),
        warehouse_count: config.warehouse_count,
        warmup_secs: config.warmup_secs,
        measure_secs: config.measure_secs,
        measure_start_ms: schedule.measure_start_ms,
        measure_end_ms: schedule.measure_end_ms,
    })?;
    write_json(&summary_path, &summary)?;
    log_driver_summary(&summary, &summary_path, &events_path);

    if let Some(coordinator_url) = &config.coordinator_url {
        submit_summary(coordinator_url, summary).await?;
    }

    Ok(())
}

async fn run_terminal(runtime: TerminalRuntime) -> Result<()> {
    let TerminalRuntime {
        config,
        client,
        metrics,
        txn_bucket_reporter,
        abort,
        start_logged,
        request_ids,
        schedule,
        run_constants,
        assignment,
        database_identity,
        seed,
    } = runtime;
    log::info!(
        "driver {} terminal {} connected to {} for warehouse {} district {}",
        config.driver_id,
        assignment.terminal_id,
        database_identity,
        assignment.warehouse_id,
        assignment.district_id
    );
    sleep_until_ms_async(schedule.warmup_start_ms).await;
    if !start_logged.swap(true, Ordering::Relaxed) {
        log::info!(
            "driver {} starting run {} with {} terminal(s)",
            config.driver_id,
            schedule.run_id,
            config.terminals()
        );
    }

    let startup_stagger_window_ms = STARTUP_STAGGER_WINDOW_MS.max(config.warmup_secs.saturating_mul(1_000) / 2);
    let startup_stagger_ms = {
        let mut startup_rng = rand::rng();
        startup_rng.random_range(0..=startup_stagger_window_ms)
    };
    if startup_stagger_ms > 0 && crate::summary::now_millis() < schedule.stop_ms {
        tokio::time::sleep(Duration::from_millis(startup_stagger_ms)).await;
    }

    let mut rng = StdRng::seed_from_u64(seed);
    let initial_kind = choose_transaction(&mut rng);
    let initial_think_delay = think_time(initial_kind, config.think_time_scale, &mut rng);
    let initial_keying_delay = keying_time(initial_kind, config.keying_time_scale);
    let synthetic_cycle_ms = u64::try_from(
        initial_think_delay
            .as_millis()
            .saturating_add(initial_keying_delay.as_millis()),
    )
    .unwrap_or(u64::MAX);
    if synthetic_cycle_ms > 0 && crate::summary::now_millis() < schedule.stop_ms {
        let phase_offset_ms = {
            let mut startup_rng = rand::rng();
            startup_rng.random_range(0..=synthetic_cycle_ms)
        };
        let remaining_ms = synthetic_cycle_ms.saturating_sub(phase_offset_ms);
        if remaining_ms > 0 {
            tokio::time::sleep(Duration::from_millis(remaining_ms)).await;
        }
    }

    let mut first_transaction = true;
    while !abort.load(Ordering::Relaxed) {
        if crate::summary::now_millis() >= schedule.stop_ms {
            break;
        }

        let kind = choose_transaction(&mut rng);
        let keying_delay = if first_transaction {
            first_transaction = false;
            let full_keying_delay = keying_time(kind, config.keying_time_scale);
            let full_keying_ms = u64::try_from(full_keying_delay.as_millis()).unwrap_or(u64::MAX);
            if full_keying_ms == 0 {
                Duration::ZERO
            } else {
                let keying_phase_offset_ms = {
                    let mut startup_rng = rand::rng();
                    startup_rng.random_range(0..=full_keying_ms)
                };
                Duration::from_millis(full_keying_ms.saturating_sub(keying_phase_offset_ms))
            }
        } else {
            keying_time(kind, config.keying_time_scale)
        };
        if !keying_delay.is_zero() && crate::summary::now_millis() < schedule.stop_ms {
            tokio::time::sleep(keying_delay).await;
        }

        if abort.load(Ordering::Relaxed) || crate::summary::now_millis() >= schedule.stop_ms {
            break;
        }

        let started_ms = crate::summary::now_millis();
        let context = TransactionContext {
            client: client.as_ref(),
            config: &config,
            run_id: &schedule.run_id,
            driver_id: &config.driver_id,
            assignment: &assignment,
            constants: &run_constants,
            request_ids: &request_ids,
        };
        let event = execute_transaction(&context, kind, &mut rng, started_ms).await;

        match event {
            Ok(record) => {
                // Some metrics depend on knowing all completed orders, even outside the
                // measurement window
                if record.kind == TransactionKind::NewOrder && record.success {
                    txn_bucket_reporter.record(record.timestamp_ms);
                }

                if record.timestamp_ms >= schedule.measure_start_ms && record.timestamp_ms < schedule.measure_end_ms {
                    metrics.record(record)?;
                }
            }
            Err(err) => {
                log::error!("terminal task error: {err:#}",);
            }
        }

        let think_delay = think_time(kind, config.think_time_scale, &mut rng);
        if !think_delay.is_zero() && crate::summary::now_millis() < schedule.stop_ms {
            tokio::time::sleep(think_delay).await;
        }
    }
    Ok(())
}

async fn execute_transaction(
    context: &TransactionContext<'_>,
    kind: TransactionKind,
    rng: &mut StdRng,
    started_ms: u64,
) -> Result<TransactionRecord> {
    match kind {
        TransactionKind::NewOrder => {
            execute_new_order(
                context.client,
                context.config.warehouse_count,
                context.assignment,
                context.constants,
                rng,
                started_ms,
            )
            .await
        }
        TransactionKind::Payment => {
            execute_payment(
                context.client,
                context.config.warehouse_count,
                context.assignment,
                context.constants,
                rng,
                started_ms,
            )
            .await
        }
        TransactionKind::OrderStatus => {
            execute_order_status(context.client, context.assignment, context.constants, rng, started_ms).await
        }
        TransactionKind::Delivery => {
            execute_delivery(
                context.client,
                context.run_id,
                context.driver_id,
                context.assignment,
                context.request_ids,
                rng,
                started_ms,
            )
            .await
        }
        TransactionKind::StockLevel => execute_stock_level(context.client, context.assignment, rng, started_ms).await,
    }
}

async fn execute_new_order(
    client: &ModuleClient,
    warehouse_count: u32,
    assignment: &TerminalAssignment,
    constants: &RunConstants,
    rng: &mut StdRng,
    started_ms: u64,
) -> Result<TransactionRecord> {
    let customer_id = customer_id(rng, constants);
    let line_count = rng.random_range(5..=15);
    let invalid_line = rng.random_bool(0.01);
    let mut order_lines = Vec::with_capacity(line_count);
    let mut remote_order_line_count = 0u32;
    for idx in 0..line_count {
        let remote = warehouse_count > 1 && rng.random_bool(0.01);
        let supply_w_id = if remote {
            remote_order_line_count += 1;
            let mut remote = assignment.warehouse_id;
            while remote == assignment.warehouse_id {
                remote = rng.random_range(1..=warehouse_count);
            }
            remote
        } else {
            assignment.warehouse_id
        };
        let item_id = if invalid_line && idx + 1 == line_count {
            ITEMS + 1
        } else {
            item_id(rng, constants)
        };
        order_lines.push(NewOrderLineInput {
            item_id,
            supply_w_id,
            quantity: rng.random_range(1..=10),
        });
    }

    let result = client
        .new_order_async(
            assignment.warehouse_id,
            assignment.district_id,
            customer_id,
            order_lines,
        )
        .await?;
    let finished_ms = crate::summary::now_millis();
    match result {
        Ok(_) => Ok(TransactionRecord {
            timestamp_ms: finished_ms,
            terminal_id: assignment.terminal_id,
            kind: TransactionKind::NewOrder,
            success: true,
            latency_ms: finished_ms.saturating_sub(started_ms),
            rollback: false,
            remote: false,
            by_last_name: false,
            order_line_count: line_count as u32,
            remote_order_line_count,
            detail: None,
        }),
        Err(message) if invalid_line => Ok(TransactionRecord {
            timestamp_ms: finished_ms,
            terminal_id: assignment.terminal_id,
            kind: TransactionKind::NewOrder,
            success: false,
            latency_ms: finished_ms.saturating_sub(started_ms),
            rollback: true,
            remote: false,
            by_last_name: false,
            order_line_count: line_count as u32,
            remote_order_line_count,
            detail: Some(message),
        }),
        Err(message) => bail!(
            "unexpected new_order failure for terminal {}: {}",
            assignment.terminal_id,
            message
        ),
    }
}

async fn execute_payment(
    client: &ModuleClient,
    warehouse_count: u32,
    assignment: &TerminalAssignment,
    constants: &RunConstants,
    rng: &mut StdRng,
    started_ms: u64,
) -> Result<TransactionRecord> {
    let remote = warehouse_count > 1 && rng.random_bool(0.15);
    let c_w_id = if remote {
        let mut other = assignment.warehouse_id;
        while other == assignment.warehouse_id {
            other = rng.random_range(1..=warehouse_count);
        }
        other
    } else {
        assignment.warehouse_id
    };
    let c_d_id = if remote {
        rng.random_range(1..=DISTRICTS_PER_WAREHOUSE)
    } else {
        assignment.district_id
    };
    let by_last_name = rng.random_bool(0.60);
    let selector = if by_last_name {
        CustomerSelector::ByLastName(customer_last_name(rng, constants))
    } else {
        CustomerSelector::ById(customer_id(rng, constants))
    };
    let amount_cents = rng.random_range(100..=500_000);
    let finished = expect_ok(
        "payment",
        client
            .payment_async(
                assignment.warehouse_id,
                assignment.district_id,
                c_w_id,
                c_d_id,
                selector,
                amount_cents,
            )
            .await,
    )?;
    let _ = finished;
    let finished_ms = crate::summary::now_millis();
    Ok(TransactionRecord {
        timestamp_ms: finished_ms,
        terminal_id: assignment.terminal_id,
        kind: TransactionKind::Payment,
        success: true,
        latency_ms: finished_ms.saturating_sub(started_ms),
        rollback: false,
        remote,
        by_last_name,
        order_line_count: 0,
        remote_order_line_count: 0,
        detail: None,
    })
}

async fn execute_order_status(
    client: &ModuleClient,
    assignment: &TerminalAssignment,
    constants: &RunConstants,
    rng: &mut StdRng,
    started_ms: u64,
) -> Result<TransactionRecord> {
    let by_last_name = rng.random_bool(0.60);
    let selector = if by_last_name {
        CustomerSelector::ByLastName(customer_last_name(rng, constants))
    } else {
        CustomerSelector::ById(customer_id(rng, constants))
    };
    let _ = expect_ok(
        "order_status",
        client
            .order_status_async(assignment.warehouse_id, assignment.district_id, selector)
            .await,
    )?;
    let finished_ms = crate::summary::now_millis();
    Ok(TransactionRecord {
        timestamp_ms: finished_ms,
        terminal_id: assignment.terminal_id,
        kind: TransactionKind::OrderStatus,
        success: true,
        latency_ms: finished_ms.saturating_sub(started_ms),
        rollback: false,
        remote: false,
        by_last_name,
        order_line_count: 0,
        remote_order_line_count: 0,
        detail: None,
    })
}

async fn execute_delivery(
    client: &ModuleClient,
    run_id: &str,
    driver_id: &str,
    assignment: &TerminalAssignment,
    request_ids: &AtomicU64,
    rng: &mut StdRng,
    started_ms: u64,
) -> Result<TransactionRecord> {
    let request_id = request_ids.fetch_add(1, Ordering::Relaxed);
    let _ = expect_ok(
        "queue_delivery",
        client
            .queue_delivery_async(
                run_id.to_string(),
                driver_id.to_string(),
                assignment.terminal_id,
                request_id,
                assignment.warehouse_id,
                rng.random_range(1..=10),
            )
            .await,
    )?;
    let finished_ms = crate::summary::now_millis();
    Ok(TransactionRecord {
        timestamp_ms: finished_ms,
        terminal_id: assignment.terminal_id,
        kind: TransactionKind::Delivery,
        success: true,
        latency_ms: finished_ms.saturating_sub(started_ms),
        rollback: false,
        remote: false,
        by_last_name: false,
        order_line_count: 0,
        remote_order_line_count: 0,
        detail: None,
    })
}

async fn execute_stock_level(
    client: &ModuleClient,
    assignment: &TerminalAssignment,
    rng: &mut StdRng,
    started_ms: u64,
) -> Result<TransactionRecord> {
    let threshold = rng.random_range(10..=20);
    let _ = expect_ok(
        "stock_level",
        client
            .stock_level_async(assignment.warehouse_id, assignment.district_id, threshold)
            .await,
    )?;
    let finished_ms = crate::summary::now_millis();
    Ok(TransactionRecord {
        timestamp_ms: finished_ms,
        terminal_id: assignment.terminal_id,
        kind: TransactionKind::StockLevel,
        success: true,
        latency_ms: finished_ms.saturating_sub(started_ms),
        rollback: false,
        remote: false,
        by_last_name: false,
        order_line_count: 0,
        remote_order_line_count: 0,
        detail: None,
    })
}

async fn resolve_driver_setup(config: DriverConfig) -> Result<(DriverConfig, RunSchedule)> {
    if let Some(coordinator_url) = &config.coordinator_url {
        const REGISTER_ATTEMPTS: u32 = 5;
        const REGISTER_RETRY_DELAY_MS: u64 = 500;
        let client = reqwest::Client::new();
        let register = RegisterDriverRequest {
            driver_id: config.driver_id.clone(),
        };
        log::info!(
            "driver {} registering with coordinator {}",
            config.driver_id,
            coordinator_url
        );
        let mut last_error = None;
        let mut response = None;
        for attempt in 1..=REGISTER_ATTEMPTS {
            match client
                .post(format!("{}/register", coordinator_url))
                .json(&register)
                .send()
                .await
            {
                Ok(http_response) => match http_response.error_for_status() {
                    Ok(http_response) => match http_response.json::<RegisterDriverResponse>().await {
                        Ok(parsed) => {
                            response = Some(parsed);
                            break;
                        }
                        Err(err) => {
                            last_error = Some(anyhow!("failed to decode register response: {err}"));
                        }
                    },
                    Err(err) => {
                        last_error = Some(anyhow!("coordinator rejected register request: {err}"));
                    }
                },
                Err(err) => {
                    last_error = Some(anyhow!("failed to register driver with coordinator: {err}"));
                }
            }

            if attempt < REGISTER_ATTEMPTS {
                log::warn!(
                    "driver {} failed to register with coordinator on attempt {}/{}; retrying in {}ms",
                    config.driver_id,
                    attempt,
                    REGISTER_ATTEMPTS,
                    REGISTER_RETRY_DELAY_MS
                );
                tokio::time::sleep(Duration::from_millis(REGISTER_RETRY_DELAY_MS)).await;
            }
        }
        let response = match response {
            Some(response) => response,
            None => {
                return Err(last_error.unwrap_or_else(|| anyhow!("driver registration failed without an error")));
            }
        };
        if !response.accepted {
            bail!("coordinator did not accept driver registration");
        }
        let Some(assignment) = response.assignment else {
            bail!("coordinator accepted driver registration without an assignment");
        };
        log::info!(
            "driver {} got assignment: warehouse_count={} warehouse_start={} driver_warehouse_count={} warehouses_per_database={}",
            config.driver_id,
            assignment.warehouse_count,
            assignment.warehouse_start,
            assignment.driver_warehouse_count,
            assignment.warehouses_per_database
        );
        let config = config.with_assignment(&assignment);
        log::info!("driver {} waiting for coordinator schedule", config.driver_id);
        loop {
            let response: ScheduleResponse = client
                .get(format!("{}/schedule", coordinator_url))
                .send()
                .await
                .context("failed to poll coordinator schedule")?
                .error_for_status()
                .context("coordinator schedule endpoint returned error")?
                .json()
                .await
                .context("failed to decode schedule response")?;
            if let Some(schedule) = response.schedule {
                log::info!(
                    "driver {} received schedule: run_id={} warmup_start_ms={} measure_start_ms={} measure_end_ms={}",
                    config.driver_id,
                    schedule.run_id,
                    schedule.warmup_start_ms,
                    schedule.measure_start_ms,
                    schedule.measure_end_ms
                );
                return Ok((config, schedule));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let run_id = config.run_id.clone().unwrap_or_else(default_run_id);
    let warmup_start_ms = crate::summary::now_millis() + 2_000;
    let measure_start_ms = warmup_start_ms + (config.warmup_secs * 1_000);
    let measure_end_ms = measure_start_ms + (config.measure_secs * 1_000);
    Ok((
        config,
        RunSchedule {
            run_id,
            warmup_start_ms,
            measure_start_ms,
            measure_end_ms,
            stop_ms: measure_end_ms,
        },
    ))
}

async fn harvest_delivery_completions(
    config: &DriverConfig,
    schedule: &RunSchedule,
    metrics: &SharedMetrics,
    shared_database_clients: &SharedDatabaseClients,
) -> Result<()> {
    let expected = metrics.delivery_queued();
    if expected == 0 {
        return Ok(());
    }
    let harvest_clients: Vec<_> = shared_database_clients
        .iter()
        .map(|(database_number, clients)| {
            let client = representative_database_client(clients).expect("database client pool should not be empty");
            (*database_number, client)
        })
        .collect();

    let mut pending_jobs = 0u64;
    let mut completed_jobs = 0u64;
    for (_, client) in &harvest_clients {
        let progress = expect_ok(
            "delivery_progress",
            client.delivery_progress_async(schedule.run_id.clone()).await,
        )?;
        pending_jobs += progress.pending_jobs;
        completed_jobs += progress.completed_jobs;
    }
    log::info!(
        "delivery progress before harvest: pending_jobs={} completed_jobs={}",
        pending_jobs,
        completed_jobs
    );
    let deadline = crate::summary::now_millis() + (config.delivery_wait_secs * 1_000);
    let mut seen_for_driver = 0u64;
    let mut after_completion_ids: BTreeMap<u32, u64> = harvest_clients
        .iter()
        .map(|(database_number, _)| (*database_number, 0))
        .collect();

    loop {
        if seen_for_driver >= expected {
            break;
        }
        let mut saw_rows = false;
        for (database_number, client) in &harvest_clients {
            let after_completion_id = after_completion_ids
                .get_mut(database_number)
                .expect("after_completion_ids should have one entry per database");
            let batch = expect_ok(
                "fetch_delivery_completions",
                client
                    .fetch_delivery_completions_async(schedule.run_id.clone(), *after_completion_id, 512)
                    .await,
            )?;
            if batch.is_empty() {
                continue;
            }
            saw_rows = true;
            for row in batch {
                *after_completion_id = (*after_completion_id).max(row.completion_id);
                if row.driver_id == config.driver_id {
                    seen_for_driver += 1;
                    metrics.record_delivery_completion(&row);
                }
            }
        }
        if !saw_rows {
            if crate::summary::now_millis() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    if seen_for_driver < expected {
        log::warn!(
            "driver {} observed only {} / {} delivery completions before timeout",
            config.driver_id,
            seen_for_driver,
            expected
        );
    }

    // It's not actually correct to shut down the clients here:
    // they may still be synchronously waiting for a response to an outstanding TX in another thread,
    // and if we shut them down it will never come, meaning they will fail and abort.
    // Instead, just let them shut down in their own time.
    // for (_, client) in clients {
    //     client.shutdown();
    // }
    Ok(())
}

fn databases_for_warehouse_slice(config: &DriverConfig) -> Vec<u32> {
    let first = (config.warehouse_start - 1) / config.warehouses_per_database;
    let last = (config.warehouse_end() - 1) / config.warehouses_per_database;
    (first..=last).collect()
}

fn describe_databases(topology: &DatabaseTopology, used_database_numbers: &[u32]) -> String {
    used_database_numbers
        .iter()
        .map(|database_number| topology.database_name(*database_number))
        .collect::<Vec<_>>()
        .join(",")
}

async fn submit_summary(coordinator_url: &str, summary: DriverSummary) -> Result<()> {
    let client = reqwest::Client::new();
    client
        .post(format!("{}/summary", coordinator_url))
        .json(&SubmitSummaryRequest { summary })
        .send()
        .await
        .context("failed to submit summary to coordinator")?
        .error_for_status()
        .context("coordinator rejected summary")?;
    Ok(())
}

fn resolve_output_dir(config: &DriverConfig, run_id: &str) -> PathBuf {
    match &config.output_dir {
        Some(path) => path.clone(),
        None => PathBuf::from("tpcc-results").join(run_id).join(&config.driver_id),
    }
}

async fn sleep_until_ms_async(target_ms: u64) {
    let now_ms = crate::summary::now_millis();
    if target_ms > now_ms {
        tokio::time::sleep(Duration::from_millis(target_ms - now_ms)).await;
    }
}

async fn connect_shared_database_clients(
    config: &DriverConfig,
    topology: &DatabaseTopology,
    used_database_numbers: &[u32],
) -> Result<SharedDatabaseClients> {
    let pool_size = config.connections_per_database;
    let mut connect_tasks = JoinSet::new();
    for database_number in used_database_numbers {
        let database_number = *database_number;
        let database_identity = topology.identity_for_database_number(database_number)?;
        let database_name = topology.database_name(database_number);
        for connection_index in 0..pool_size {
            log::info!(
                "driver {} starting shared client connection {}/{} to {} ({})",
                config.driver_id,
                connection_index + 1,
                pool_size,
                database_name,
                database_identity
            );
            let connection = config.connection.clone();
            let database_name = database_name.clone();
            connect_tasks.spawn(async move {
                let client = ModuleClient::connect_async(&connection, database_identity)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to connect shared client {}/{} to {database_name}",
                            connection_index + 1,
                            pool_size
                        )
                    })?;
                Ok::<_, anyhow::Error>((database_number, database_name, connection_index, Arc::new(client)))
            });
        }
    }

    let mut shared_clients: BTreeMap<u32, Vec<(usize, Arc<ModuleClient>)>> = used_database_numbers
        .iter()
        .copied()
        .map(|database_number| (database_number, Vec::with_capacity(pool_size)))
        .collect();
    while let Some(result) = connect_tasks.join_next().await {
        match result {
            Ok(Ok((database_number, database_name, connection_index, client))) => {
                log::info!(
                    "driver {} shared database client connection {}/{} connected to {}",
                    config.driver_id,
                    connection_index + 1,
                    pool_size,
                    database_name
                );
                shared_clients
                    .get_mut(&database_number)
                    .expect("shared client pool should exist for database")
                    .push((connection_index, client));
            }
            Ok(Err(err)) => {
                log::error!(
                    "driver {} failed to connect a shared database client: {err:#}",
                    config.driver_id
                );
                connect_tasks.abort_all();
                return Err(err);
            }
            Err(err) => {
                log::error!(
                    "driver {} failed to connect a shared database client: {err:#}",
                    config.driver_id
                );
                connect_tasks.abort_all();
                return Err(anyhow!("shared database connection task failed: {}", err));
            }
        }
    }
    Ok(shared_clients
        .into_iter()
        .map(|(database_number, mut clients)| {
            clients.sort_by_key(|(connection_index, _)| *connection_index);
            (
                database_number,
                clients.into_iter().map(|(_, client)| client).collect::<Vec<_>>(),
            )
        })
        .collect())
}

async fn connect_metrics_clients(config: &DriverConfig) -> Result<MetricsClientPool> {
    let pool_size = config.connections_per_database;
    let mut connect_tasks = JoinSet::new();
    for connection_index in 0..pool_size {
        log::info!(
            "driver {} starting metrics client connection {}/{}",
            config.driver_id,
            connection_index + 1,
            pool_size
        );
        let connection = config.connection.clone();
        connect_tasks.spawn(async move {
            let client = connect_metrics_module_async(&connection).await.with_context(|| {
                format!(
                    "failed to connect metrics client {}/{}",
                    connection_index + 1,
                    pool_size
                )
            })?;
            Ok::<_, anyhow::Error>((connection_index, Arc::new(client)))
        });
    }

    let mut metrics_clients = Vec::with_capacity(pool_size);
    while let Some(result) = connect_tasks.join_next().await {
        match result {
            Ok(Ok((connection_index, client))) => {
                log::info!(
                    "driver {} metrics client connection {}/{} connected",
                    config.driver_id,
                    connection_index + 1,
                    pool_size
                );
                metrics_clients.push((connection_index, client));
            }
            Ok(Err(err)) => {
                log::error!(
                    "driver {} failed to connect a metrics client: {err:#}",
                    config.driver_id
                );
                connect_tasks.abort_all();
                return Err(err);
            }
            Err(err) => {
                log::error!(
                    "driver {} failed to connect a metrics client: {err:#}",
                    config.driver_id
                );
                connect_tasks.abort_all();
                return Err(anyhow!("metrics connection task failed: {}", err));
            }
        }
    }

    metrics_clients.sort_by_key(|(connection_index, _)| *connection_index);
    Ok(metrics_clients.into_iter().map(|(_, client)| client).collect())
}

impl TxnBucketReporter {
    fn new(run_start_ms: u64, metrics_clients: MetricsClientPool) -> Self {
        Self {
            run_start_ms,
            pending: Mutex::new(BTreeMap::new()),
            metrics_clients,
            next_client: AtomicU64::new(0),
        }
    }

    fn record(&self, timestamp_ms: u64) {
        let bucket_start_ms = bucket_start_ms(self.run_start_ms, timestamp_ms);
        let mut pending = self.pending.lock().expect("txn bucket reporter mutex poisoned");
        let count = pending.entry(bucket_start_ms).or_insert(0);
        *count = count.saturating_add(1);
    }

    async fn flush(&self) -> Result<()> {
        let drained = {
            let mut pending = self.pending.lock().expect("txn bucket reporter mutex poisoned");
            if pending.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *pending)
        };

        if let Err(err) = self.send_counts(&drained).await {
            let mut pending = self.pending.lock().expect("txn bucket reporter mutex poisoned");
            for (bucket_start_ms, count) in drained {
                let pending_count = pending.entry(bucket_start_ms).or_insert(0);
                *pending_count = pending_count.saturating_add(count);
            }
            return Err(err);
        }

        Ok(())
    }

    async fn send_counts(&self, counts: &BTreeMap<u64, u64>) -> Result<()> {
        for (bucket_start_ms, count) in counts {
            let client = self.next_metrics_client().context("missing metrics client")?;
            client
                .reducers
                .record_txn_bucket_count(*bucket_start_ms, *count)
                .with_context(|| {
                    format!(
                        "failed to send txn bucket count bucket_start_ms={} count={}",
                        bucket_start_ms, count
                    )
                })?;
        }
        Ok(())
    }

    fn next_metrics_client(&self) -> Option<&Arc<MetricsDbConnection>> {
        if self.metrics_clients.is_empty() {
            return None;
        }
        let index = self.next_client.fetch_add(1, Ordering::Relaxed);
        let index = usize::try_from(index).unwrap_or(usize::MAX) % self.metrics_clients.len();
        self.metrics_clients.get(index)
    }
}

fn spawn_txn_bucket_reporter(reporter: Arc<TxnBucketReporter>, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(METRICS_FLUSH_INTERVAL_MS)).await;
            if let Err(err) = reporter.flush().await {
                log::warn!("failed to flush txn bucket metrics; will retry: {err:#}");
            }
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
        }
    })
}

async fn stop_txn_bucket_reporter(
    reporter: &TxnBucketReporter,
    shutdown: Arc<AtomicBool>,
    reporter_task: JoinHandle<()>,
) -> Result<()> {
    shutdown.store(true, Ordering::Relaxed);
    reporter_task.await.context("txn bucket reporter task failed")?;
    reporter.flush().await.context("failed final txn bucket metrics flush")
}

fn bucket_start_ms(run_start_ms: u64, timestamp_ms: u64) -> u64 {
    let bucket_offset_ms = timestamp_ms.saturating_sub(run_start_ms);
    run_start_ms + ((bucket_offset_ms / 1_000) * 1_000)
}

async fn shutdown_metrics_clients(metrics_clients: MetricsClientPool) {
    for client in metrics_clients {
        if let Some(client) = Arc::into_inner(client) {
            let _ = client.disconnect();
        }
    }
}

async fn shutdown_shared_database_clients(shared_database_clients: SharedDatabaseClients) {
    for (_, clients) in shared_database_clients {
        for client in clients {
            if let Some(client) = Arc::into_inner(client) {
                client.shutdown_async().await;
            }
        }
    }
}

fn select_pooled_database_client(
    shared_database_clients: &SharedDatabaseClients,
    database_number: u32,
    terminal_id: u32,
) -> Option<&Arc<ModuleClient>> {
    let clients = shared_database_clients.get(&database_number)?;
    select_pooled_client(clients, terminal_id)
}

fn representative_database_client(clients: &DatabaseClientPool) -> Option<&Arc<ModuleClient>> {
    clients.first()
}

fn total_shared_database_clients<T>(shared_database_clients: &BTreeMap<u32, Vec<T>>) -> usize {
    shared_database_clients.values().map(Vec::len).sum()
}

fn pooled_client_index(pool_size: usize, terminal_id: u32) -> Option<usize> {
    if pool_size == 0 {
        return None;
    }
    Some(usize::try_from(terminal_id).unwrap_or(usize::MAX) % pool_size)
}

fn select_pooled_client<T>(clients: &[Arc<T>], terminal_id: u32) -> Option<&Arc<T>> {
    let index = pooled_client_index(clients.len(), terminal_id)?;
    clients.get(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pooled_client_index_uses_deterministic_modulo() {
        assert_eq!(pooled_client_index(3, 10), Some(1));
        assert_eq!(pooled_client_index(3, 13), Some(1));
    }

    #[test]
    fn pooled_client_index_rejects_empty_pool() {
        assert_eq!(pooled_client_index(0, 1), None);
    }

    #[test]
    fn total_shared_database_clients_counts_all_pooled_connections() {
        let clients: BTreeMap<u32, Vec<usize>> = [(1, vec![0]), (2, vec![0, 1])].into_iter().collect();
        assert_eq!(total_shared_database_clients(&clients), 3);
    }

    #[test]
    fn select_pooled_client_uses_deterministic_modulo() {
        let metrics_clients = vec![Arc::new(()), Arc::new(()), Arc::new(())];
        let selected = select_pooled_client(&metrics_clients, 10).expect("client");
        assert!(Arc::ptr_eq(selected, &metrics_clients[1]));
    }

    #[test]
    fn bucket_start_ms_snaps_to_run_relative_second() {
        assert_eq!(bucket_start_ms(1_234, 1_234), 1_234);
        assert_eq!(bucket_start_ms(1_234, 2_233), 1_234);
        assert_eq!(bucket_start_ms(1_234, 2_234), 2_234);
    }
}
