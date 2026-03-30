use anyhow::{anyhow, bail, Context, Result};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use crate::client::{expect_ok, ModuleClient};
use crate::config::{default_run_id, DriverConfig};
use crate::metrics_module_bindings::register_completed_order;
use crate::metrics_module_client::connect_metrics_module_async;
use crate::module_bindings::*;
use crate::protocol::{
    RegisterDriverRequest, RegisterDriverResponse, RunSchedule, ScheduleResponse, SubmitSummaryRequest,
};
use crate::summary::{
    log_driver_summary, write_json, DriverSummary, DriverSummaryMeta, SharedMetrics, TransactionKind,
    TransactionRecord,
};
use crate::topology::DatabaseTopology;
use crate::tpcc::*;

struct TerminalRuntime {
    config: DriverConfig,
    metrics: SharedMetrics,
    abort: Arc<AtomicBool>,
    start_logged: Arc<AtomicBool>,
    request_ids: Arc<AtomicU64>,
    schedule: RunSchedule,
    run_constants: RunConstants,
    assignment: TerminalAssignment,
    database_identity: spacetimedb_sdk::Identity,
    seed: u64,
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
    fs::create_dir_all(&output_dir).with_context(|| format!("failed to create {}", output_dir.display()))?;
    let topology = DatabaseTopology::for_driver(&config).await?;
    let used_database_numbers = databases_for_warehouse_slice(&config);
    let database_summary = describe_databases(&topology, &used_database_numbers);

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

    for warehouse_id in config.warehouse_start..=config.warehouse_end() {
        let database_identity = topology.identity_for_warehouse(warehouse_id)?;
        for district_id in 1..=DISTRICTS_PER_WAREHOUSE {
            let assignment = TerminalAssignment {
                terminal_id: terminal_id(warehouse_id, district_id),
                warehouse_id,
                district_id,
            };
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
                metrics: terminal_metrics,
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
                    first_error = Some(err);
                }
            }
            Err(err) => {
                abort.store(true, Ordering::Relaxed);
                if first_error.is_none() {
                    first_error = Some(anyhow!("terminal task failed: {}", err));
                }
            }
        }
    }
    if let Some(err) = first_error {
        return Err(err);
    }

    harvest_delivery_completions(&config, &schedule, &metrics, &topology, &used_database_numbers).await?;

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
        metrics,
        abort,
        start_logged,
        request_ids,
        schedule,
        run_constants,
        assignment,
        database_identity,
        seed,
    } = runtime;
    let client = ModuleClient::connect_async(&config.connection, database_identity).await?;
    let metrics_client = connect_metrics_module_async(&config.connection).await?;
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

    let mut rng = StdRng::seed_from_u64(seed);
    while !abort.load(Ordering::Relaxed) {
        if crate::summary::now_millis() >= schedule.stop_ms {
            break;
        }

        let kind = choose_transaction(&mut rng);
        let started_ms = crate::summary::now_millis();
        let context = TransactionContext {
            client: &client,
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
                    let _ = metrics_client.reducers.register_completed_order();
                }

                if record.timestamp_ms >= schedule.measure_start_ms && record.timestamp_ms < schedule.measure_end_ms {
                    metrics.record(record)?;
                }
            }
            Err(err) => {
                abort.store(true, Ordering::Relaxed);
                client.shutdown_async().await;
                return Err(err);
            }
        }

        let delay = keying_time(kind, config.keying_time_scale) + think_time(kind, config.think_time_scale, &mut rng);
        if !delay.is_zero() && crate::summary::now_millis() < schedule.stop_ms {
            tokio::time::sleep(delay).await;
        }
    }

    client.shutdown_async().await;
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
                return Err(last_error
                    .unwrap_or_else(|| anyhow!("driver registration failed without an error")));
            }
        };
        if !response.accepted {
            bail!("coordinator did not accept driver registration");
        }
        let Some(assignment) = response.assignment else {
            bail!("coordinator accepted driver registration without an assignment");
        };
        let config = config.with_assignment(&assignment);
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
    topology: &DatabaseTopology,
    used_database_numbers: &[u32],
) -> Result<()> {
    let expected = metrics.delivery_queued();
    if expected == 0 {
        return Ok(());
    }
    let mut harvest_clients = Vec::with_capacity(used_database_numbers.len());
    for database_number in used_database_numbers {
        let database_identity = topology.identity_for_database_number(*database_number)?;
        let client = ModuleClient::connect_async(&config.connection, database_identity)
            .await
            .with_context(|| {
                format!(
                    "failed to connect delivery harvester to {}",
                    topology.database_name(*database_number)
                )
            })?;
        harvest_clients.push((*database_number, client));
    }

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
    let mut after_completion_ids = vec![0u64; harvest_clients.len()];

    loop {
        if seen_for_driver >= expected {
            break;
        }
        let mut saw_rows = false;
        for ((_, client), after_completion_id) in harvest_clients.iter().zip(after_completion_ids.iter_mut()) {
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
