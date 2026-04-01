use anyhow::{Context, Result};
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::config::CoordinatorConfig;
use crate::metrics_module_bindings::reset_reducer::reset;
use crate::metrics_module_client::connect_metrics_module_async;
use crate::protocol::{
    DriverAssignment, RegisterDriverRequest, RegisterDriverResponse, RunSchedule, ScheduleResponse,
    SubmitSummaryRequest,
};
use crate::summary::{
    aggregate_summaries, log_aggregate_summary, now_millis, write_json, AggregateSummary, DriverSummary,
};

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<CoordinatorState>>,
}

struct CoordinatorState {
    config: CoordinatorConfig,
    registrations: BTreeMap<String, DriverRegistration>,
    registration_order: Vec<String>,
    schedule: Option<RunSchedule>,
    summaries: BTreeMap<String, DriverSummary>,
}

struct DriverRegistration {
    assignment: DriverAssignment,
}

pub async fn run(config: CoordinatorConfig) -> Result<()> {
    fs::create_dir_all(&config.output_dir)
        .with_context(|| format!("failed to create {}", config.output_dir.display()))?;

    let state = AppState {
        inner: Arc::new(Mutex::new(CoordinatorState {
            config: config.clone(),
            registrations: BTreeMap::new(),
            registration_order: Vec::new(),
            schedule: None,
            summaries: BTreeMap::new(),
        })),
    };

    let app = Router::new()
        .route("/register", post(register_driver))
        .route("/schedule", get(get_schedule))
        .route("/summary", post(submit_summary))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("failed to bind {}", config.listen))?;
    log::info!("coordinator listening on {}", config.listen);
    axum::serve(listener, app).await.context("coordinator server exited")
}

async fn register_driver(
    State(state): State<AppState>,
    Json(request): Json<RegisterDriverRequest>,
) -> Json<RegisterDriverResponse> {
    let (assignment, is_new_registration, registered, expected_drivers) = {
        let mut inner = state.inner.lock();
        let expected_drivers = inner.config.expected_drivers;
        match inner.registrations.get(&request.driver_id) {
            Some(existing) => {
                let registered = inner.registrations.len();
                (existing.assignment.clone(), false, registered, expected_drivers)
            }
            None => {
                if inner.registration_order.len() >= expected_drivers {
                    return Json(RegisterDriverResponse {
                        accepted: false,
                        assignment: None,
                    });
                }
                let index = inner.registration_order.len();
                let assignment = assignment_for_index(&inner.config, index);
                inner.registration_order.push(request.driver_id.clone());
                inner.registrations.insert(
                    request.driver_id.clone(),
                    DriverRegistration {
                        assignment: assignment.clone(),
                    },
                );
                let registered = inner.registrations.len();
                (assignment, true, registered, expected_drivers)
            }
        }
    };
    maybe_create_schedule(&state).await;
    let warehouse_end = assignment_end(&assignment);
    if is_new_registration {
        log::info!(
            "driver {} registered and ready ({}/{}): warehouses {}..={} ({} warehouse(s))",
            request.driver_id,
            registered,
            expected_drivers,
            assignment.warehouse_start,
            warehouse_end,
            assignment.driver_warehouse_count
        );
    } else {
        log::info!(
            "driver {} re-registered and remains ready: warehouses {}..={}",
            request.driver_id,
            assignment.warehouse_start,
            warehouse_end
        );
    }
    Json(RegisterDriverResponse {
        accepted: true,
        assignment: Some(assignment),
    })
}

async fn get_schedule(State(state): State<AppState>) -> Json<ScheduleResponse> {
    let inner = state.inner.lock();
    Json(ScheduleResponse {
        ready: inner.schedule.is_some(),
        schedule: inner.schedule.clone(),
    })
}

async fn submit_summary(
    State(state): State<AppState>,
    Json(request): Json<SubmitSummaryRequest>,
) -> Result<Json<AggregateSummary>, axum::http::StatusCode> {
    let aggregate = {
        let mut inner = state.inner.lock();
        let replaced = inner
            .summaries
            .insert(request.summary.driver_id.clone(), request.summary.clone())
            .is_some();
        let received = inner.summaries.len();
        log::info!(
            "received summary from driver {} ({}/{}{})",
            request.summary.driver_id,
            received,
            inner.config.expected_drivers,
            if replaced { ", replaced existing summary" } else { "" }
        );
        if received == inner.config.expected_drivers {
            let summaries: Vec<_> = inner.summaries.values().cloned().collect();
            let aggregate = aggregate_summaries(inner.config.run_id.clone(), &summaries);
            let summary_path = aggregate_summary_path(&inner.config.output_dir, &aggregate);
            if let Err(err) = write_aggregate(&inner.config.output_dir, &aggregate) {
                log::error!("failed to write aggregate summary: {err:#}");
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
            log::info!(
                "received all {} summary(s) for run {}",
                inner.config.expected_drivers,
                inner.config.run_id
            );
            log_aggregate_summary(&aggregate, &summary_path);
            aggregate
        } else {
            aggregate_summaries(
                inner.config.run_id.clone(),
                &inner.summaries.values().cloned().collect::<Vec<_>>(),
            )
        }
    };
    Ok(Json(aggregate))
}

async fn maybe_create_schedule(state: &AppState) {
    // Check whether schedule creation is needed, and grab config, without holding the lock
    // during the async metrics module connection below.
    let config = {
        let inner = state.inner.lock();
        if inner.schedule.is_some() || inner.registrations.len() < inner.config.expected_drivers {
            return;
        }
        inner.config.clone()
    };

    let warmup_start_ms = now_millis() + 2_000;
    let measure_start_ms = warmup_start_ms + (config.warmup_secs * 1_000);
    let measure_end_ms = measure_start_ms + (config.measure_secs * 1_000);

    let metrics_client = match connect_metrics_module_async(&config.connection).await {
        Ok(client) => client,
        Err(e) => {
            log::error!("failed to connect to metrics module: {e:#}");
            return;
        }
    };
    let _ = metrics_client.reducers.reset(
        config.warehouses as u64,
        config.warmup_secs * 1000,
        measure_start_ms,
        measure_end_ms,
    );

    let schedule = RunSchedule {
        run_id: config.run_id.clone(),
        warmup_start_ms,
        measure_start_ms,
        measure_end_ms,
        stop_ms: measure_end_ms,
    };

    let mut inner = state.inner.lock();
    if inner.schedule.is_some() {
        // Another concurrent registration call already created the schedule.
        return;
    }
    inner.schedule = Some(schedule.clone());
    log::info!(
        "all {} driver(s) registered; schedule ready for run {} (warmup_start_ms={} measure_start_ms={} measure_end_ms={})",
        config.expected_drivers,
        config.run_id,
        warmup_start_ms,
        measure_start_ms,
        measure_end_ms
    );
    drop(inner);
    tokio::spawn(log_schedule_events(schedule));
}

fn assignment_for_index(config: &CoordinatorConfig, index: usize) -> DriverAssignment {
    let total_warehouses = usize::try_from(config.warehouses).unwrap_or(usize::MAX);
    let expected_drivers = config.expected_drivers;
    let base = total_warehouses / expected_drivers;
    let remainder = total_warehouses % expected_drivers;
    let driver_warehouse_count = base + (index < remainder) as usize;
    let warehouse_start = 1 + (index * base) + index.min(remainder);

    DriverAssignment {
        warehouse_count: config.warehouses,
        warehouses_per_database: config.warehouses_per_database,
        warehouse_start: warehouse_start as u32,
        driver_warehouse_count: driver_warehouse_count as u32,
    }
}

fn write_aggregate(output_dir: &Path, aggregate: &AggregateSummary) -> Result<()> {
    let summary_path = aggregate_summary_path(output_dir, aggregate);
    if let Some(run_dir) = summary_path.parent() {
        fs::create_dir_all(run_dir).with_context(|| format!("failed to create {}", run_dir.display()))?;
    }
    write_json(&summary_path, aggregate)
}

fn aggregate_summary_path(output_dir: &Path, aggregate: &AggregateSummary) -> std::path::PathBuf {
    output_dir.join(&aggregate.run_id).join("summary.json")
}

fn assignment_end(assignment: &DriverAssignment) -> u32 {
    assignment
        .warehouse_start
        .saturating_add(assignment.driver_warehouse_count.saturating_sub(1))
}

async fn log_schedule_events(schedule: RunSchedule) {
    sleep_until_ms(schedule.warmup_start_ms).await;
    log::info!("run {} warmup started", schedule.run_id);
    sleep_until_ms(schedule.measure_start_ms).await;
    log::info!("run {} measurement started", schedule.run_id);
    sleep_until_ms(schedule.measure_end_ms).await;
    log::info!("run {} measurement ended", schedule.run_id);
}

async fn sleep_until_ms(target_ms: u64) {
    let now_ms = now_millis();
    if target_ms > now_ms {
        tokio::time::sleep(Duration::from_millis(target_ms - now_ms)).await;
    }
}
