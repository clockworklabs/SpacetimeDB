use crate::eval::{Lang, ScoreDetails};
use crate::llm::types::Vendor;
use crate::llm::{LlmProvider, ModelRoute};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Parameters for publishing a module (golden or LLM-generated).
pub struct PublishParams<'a> {
    pub lang: Lang,
    pub category: &'a str,
    pub task_id: &'a str,
    pub route_tag: &'a str,
    pub source_text: &'a str,
    pub db_name: String,
    pub host: Option<String>,
}

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
    pub answers_typescript: PathBuf,
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

pub struct RunContext<'a> {
    pub lang_name: &'a str,
    pub lang: Lang,
    pub route: &'a ModelRoute,
    pub context: &'a str,
    pub hash: &'a str,
    pub llm: &'a dyn LlmProvider,
    pub host: Option<String>,
}

impl<'a> RunContext<'a> {
    pub fn new(
        lang_name: &'a str,
        lang: Lang,
        route: &'a ModelRoute,
        context: &'a str,
        hash: &'a str,
        llm: &'a dyn LlmProvider,
        host: Option<String>,
    ) -> Self {
        Self {
            lang_name,
            lang,
            route,
            context,
            hash,
            llm,
            host,
        }
    }
}

pub struct BenchRunContext<'a> {
    pub bench_root: &'a Path,
    pub mode: &'a str,
    pub hash: &'a str,
    pub route: &'a ModelRoute,
    pub context: &'a str,
    pub llm: &'a dyn LlmProvider,
    pub lang: Lang,
    pub selectors: Option<&'a [String]>,
    pub host: Option<String>,
    pub details_path: PathBuf,
}

pub struct RunConfig {
    pub modes: Option<Vec<String>>,
    pub hash_only: bool,
    pub goldens_only: bool,
    pub lang: Lang,
    pub providers_filter: Option<HashSet<Vendor>>,
    pub selectors: Option<Vec<String>>,
    pub force: bool,
    pub categories: Option<HashSet<String>>,
    pub model_filter: Option<HashMap<Vendor, HashSet<String>>>,
    pub host: Option<String>,
    /// Path to the details.json file where results will be merged
    pub details_path: PathBuf,
}
