use serde::{Deserialize, Serialize};

use crate::summary::DriverSummary;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DriverAssignment {
    pub warehouse_count: u32,
    pub warehouses_per_database: u32,
    pub warehouse_start: u32,
    pub driver_warehouse_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterDriverRequest {
    pub driver_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterDriverResponse {
    pub accepted: bool,
    pub assignment: Option<DriverAssignment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSchedule {
    pub run_id: String,
    pub warmup_start_ms: u64,
    pub measure_start_ms: u64,
    pub measure_end_ms: u64,
    pub stop_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduleResponse {
    pub ready: bool,
    pub schedule: Option<RunSchedule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitSummaryRequest {
    pub summary: DriverSummary,
}
