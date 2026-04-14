use crate::bench::RunOutcome;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Results {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    pub languages: Vec<LangEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangEntry {
    pub lang: String,
    pub modes: Vec<ModeEntry>,
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
