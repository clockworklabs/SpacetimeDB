use crate::eval::ScoreDetails;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunOutcome {
    pub hash: String,
    pub task: String,
    pub lang: String,
    pub golden_published: bool,
    pub model_name: String,
    pub total_tests: u32,
    pub passed_tests: u32,

    pub llm_output: Option<String>,
    pub category: Option<String>,
    pub route_api_model: Option<String>,
    pub golden_db: Option<String>,
    pub llm_db: Option<String>,
    pub work_dir_golden: Option<String>,
    pub work_dir_llm: Option<String>,
    pub scorer_details: Option<HashMap<String, ScoreDetails>>,

    #[serde(default)]
    pub vendor: String,

    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub struct TaskPaths {
    pub root: PathBuf,
    pub answers_csharp: PathBuf,
    pub answers_rust: PathBuf,
}

pub struct RouteRun {
    pub route_name: String,
    pub api_model: String,
    pub outcomes: Vec<RunOutcome>,
}

#[derive(Debug, Error)]
pub enum RunOneError {
    #[error("{msg}")]
    WithOutput { msg: String, llm_output: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
