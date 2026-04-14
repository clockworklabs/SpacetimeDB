use crate::bench::RunOutcome;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// -- RESULTS --

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Results {
    /// ISO 8601 timestamp of when this results file was last updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
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
