use crate::bench::RunOutcome;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// -- RESULTS (details.json) --

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Results {
    pub languages: Vec<LangEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangEntry {
    pub lang: String,
    pub modes: Vec<ModeEntry>,
    #[serde(default)]
    pub golden_answers: BTreeMap<String, GoldenAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeEntry {
    pub mode: String,
    pub hash: Option<String>,
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub route_api_model: Option<String>,
    pub tasks: BTreeMap<String, RunOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoldenAnswer {
    pub answer: String,
    #[serde(default)]
    pub syntax: Option<String>, // "rust" | "csharp"
}

// -- SUMMARY (summary.json) --
#[derive(Debug, Serialize, Deserialize)]
pub struct Summary {
    pub version: u32,
    pub generated_at: String,
    pub by_language: BTreeMap<String, LangSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LangSummary {
    pub modes: BTreeMap<String, ModeSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModeSummary {
    pub hash: String,
    pub models: BTreeMap<String, ModelSummary>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Totals {
    pub tasks: u32,
    pub total_tests: u32,
    pub passed_tests: u32,
    pub pass_pct: f32,

    // sum of (passed_tests / total_tests) across tasks
    pub task_pass_equiv: f32,
    // task_pass_equiv / tasks * 100
    pub task_pass_pct: f32,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ModelSummary {
    pub categories: BTreeMap<String, CategorySummary>,
    pub totals: Totals,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CategorySummary {
    pub tasks: u32,
    pub total_tests: u32,
    pub passed_tests: u32,
    pub pass_pct: f32,

    // sum of (passed_tests / total_tests) for tasks in this category
    pub task_pass_equiv: f32,
    // task_pass_equiv / tasks * 100
    pub task_pass_pct: f32,
}
