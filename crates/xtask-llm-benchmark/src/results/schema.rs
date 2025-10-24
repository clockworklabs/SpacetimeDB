use crate::bench::RunOutcome;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(serde::Serialize, Deserialize)]
pub struct ModelRun {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
}

#[derive(serde::Serialize, Deserialize)]
pub struct ModeRun {
    pub mode: String,
    pub lang: String,
    pub hash: String,
    pub models: Vec<ModelRun>,
}

#[derive(serde::Serialize, Deserialize, Default)]
pub struct BenchmarkRun {
    pub version: u32,
    pub generated_at: String,
    pub modes: Vec<ModeRun>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelScore {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default)]
    pub details: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModeResult {
    pub mode: String,
    pub lang: String,
    pub hash: String,
    #[serde(default)]
    pub models: Vec<ModelScore>,
}

// -- RESULTS --

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Results {
    pub languages: Vec<LangEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangEntry {
    pub lang: String,
    pub modes: Vec<ModeEntry>,
    #[serde(default)]
    pub golden_answers: std::collections::HashMap<String, GoldenAnswer>,
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
    pub tasks: HashMap<String, RunOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoldenAnswer {
    pub answer: String,
    #[serde(default)]
    pub syntax: Option<String>, // "rust" | "csharp"
}

// -- SUMMARY --

#[derive(Debug, Serialize)]
pub struct Summary {
    pub version: u32,
    pub generated_at: String,
    pub by_language: HashMap<String, LangSummary>,
}

#[derive(Debug, Serialize)]
pub struct LangSummary {
    pub modes: HashMap<String, ModeSummary>,
}

#[derive(Debug, Serialize)]
pub struct ModeSummary {
    pub models: HashMap<String, ModelSummary>,
}

#[derive(Debug, Serialize)]
pub struct ModelSummary {
    pub categories: HashMap<String, CategorySummary>,
    pub totals: Totals,
}

#[derive(Debug, Serialize, Default, Clone)]
pub struct CategorySummary {
    pub tasks: u32,
    pub total_tests: u32,
    pub passed_tests: u32,
    pub pass_pct: f32,
}

#[derive(Debug, Serialize, Default, Clone)]
pub struct Totals {
    pub tasks: u32,
    pub total_tests: u32,
    pub passed_tests: u32,
    pub pass_pct: f32,
}
