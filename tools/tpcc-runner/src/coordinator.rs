use anyhow::{Context, Result};
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::CoordinatorConfig;
use crate::protocol::{
    DriverAssignment, RegisterDriverRequest, RegisterDriverResponse, RunSchedule, ScheduleResponse,
    SubmitSummaryRequest,
};
use crate::summary::{aggregate_summaries, now_millis, write_json, AggregateSummary, DriverSummary};

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
    let mut inner = state.inner.lock();
    let assignment = match inner.registrations.get(&request.driver_id) {
        Some(existing) => existing.assignment.clone(),
        None => {
            if inner.registration_order.len() >= inner.config.expected_drivers {
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
                DriverRegistration { assignment: assignment.clone() },
            );
            assignment
        }
    };
    maybe_create_schedule(&mut inner);
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
        inner
            .summaries
            .insert(request.summary.driver_id.clone(), request.summary.clone());
        if inner.summaries.len() == inner.config.expected_drivers {
            let summaries: Vec<_> = inner.summaries.values().cloned().collect();
            let aggregate = aggregate_summaries(inner.config.run_id.clone(), &summaries);
            if let Err(err) = write_aggregate(&inner.config.output_dir, &aggregate) {
                log::error!("failed to write aggregate summary: {err:#}");
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
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

fn maybe_create_schedule(inner: &mut CoordinatorState) {
    if inner.schedule.is_some() || inner.registrations.len() < inner.config.expected_drivers {
        return;
    }
    let warmup_start_ms = now_millis() + 2_000;
    let measure_start_ms = warmup_start_ms + (inner.config.warmup_secs * 1_000);
    let measure_end_ms = measure_start_ms + (inner.config.measure_secs * 1_000);
    inner.schedule = Some(RunSchedule {
        run_id: inner.config.run_id.clone(),
        warmup_start_ms,
        measure_start_ms,
        measure_end_ms,
        stop_ms: measure_end_ms,
    });
    log::info!(
        "all {} driver(s) registered; schedule ready for run {}",
        inner.config.expected_drivers,
        inner.config.run_id
    );
}

fn assignment_for_index(config: &CoordinatorConfig, index: usize) -> DriverAssignment {
    let total_warehouses = usize::from(config.warehouses);
    let expected_drivers = config.expected_drivers;
    let base = total_warehouses / expected_drivers;
    let remainder = total_warehouses % expected_drivers;
    let driver_warehouse_count = base + usize::from(index < remainder);
    let warehouse_start = 1 + (index * base) + index.min(remainder);

    DriverAssignment {
        warehouse_count: config.warehouses,
        warehouses_per_database: config.warehouses_per_database,
        warehouse_start: warehouse_start as u16,
        driver_warehouse_count: driver_warehouse_count as u16,
    }
}

fn write_aggregate(output_dir: &Path, aggregate: &AggregateSummary) -> Result<()> {
    let run_dir = output_dir.join(&aggregate.run_id);
    fs::create_dir_all(&run_dir).with_context(|| format!("failed to create {}", run_dir.display()))?;
    write_json(&run_dir.join("summary.json"), aggregate)
}
