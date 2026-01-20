use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub enum BuildTool {
    SpacetimeRust { extra_args: Vec<&'static str> },
    Dotnet { configuration: &'static str },
    CargoCheck,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScoreDetails {
    pub pass: bool,
    pub partial: f32,
    pub notes: Value,
}

impl ScoreDetails {
    /// Extract a human-readable failure reason from the notes.
    pub fn failure_reason(&self) -> Option<String> {
        if self.pass {
            return None;
        }

        let notes = self.notes.as_object()?;

        // Check for error message (publish/compile errors, timeouts)
        if let Some(err) = notes.get("error").and_then(|v| v.as_str()) {
            let short = if err.len() > 150 { &err[..150] } else { err };
            return Some(short.to_string());
        }

        // Check for table diff (schema_parity scorer)
        if let Some(tables_diff) = notes.get("tables_diff") {
            if !tables_diff.is_null() {
                if let Ok(diff) = serde_json::from_value::<SchemaDiff>(tables_diff.clone()) {
                    if !diff.only_golden.is_empty() || !diff.only_llm.is_empty() {
                        let golden_names: Vec<_> = diff.only_golden.keys().collect();
                        let llm_names: Vec<_> = diff.only_llm.keys().collect();
                        return Some(format!(
                            "tables differ - expected {:?}, got {:?}",
                            golden_names, llm_names
                        ));
                    }
                }
            }
        }

        // Check for reducer diff
        if let Some(reducers_diff) = notes.get("reducers_diff") {
            if !reducers_diff.is_null() {
                if let Ok(diff) = serde_json::from_value::<ReducerDiff>(reducers_diff.clone()) {
                    if !diff.only_golden.is_empty() || !diff.only_llm.is_empty() {
                        return Some(format!(
                            "reducers differ - expected {:?}, got {:?}",
                            diff.only_golden, diff.only_llm
                        ));
                    }
                }
            }
        }

        Some("failed".to_string())
    }
}

/// Diff structure for table comparisons in schema_parity scorer.
#[derive(Debug, Deserialize, Default)]
pub struct SchemaDiff {
    #[serde(default)]
    pub only_golden: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    pub only_llm: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    pub changed: std::collections::BTreeMap<String, Value>,
}

/// Diff structure for reducer comparisons.
#[derive(Debug, Deserialize, Default)]
pub struct ReducerDiff {
    #[serde(default)]
    pub only_golden: Vec<String>,
    #[serde(default)]
    pub only_llm: Vec<String>,
}

pub struct ReducerDataParityConfig<'a> {
    pub src_file: &'a str,
    pub route_tag: &'a str,
    pub reducer: String,
    pub args: Vec<Value>,
    pub select_query: String,
    pub id_str: &'static str,
    pub collapse_ws: bool,
    pub timeout: Duration,
}

pub struct ReducerSqlCountConfig<'a> {
    pub src_file: &'a str,
    pub route_tag: &'a str,
    pub reducer: String,
    pub args: Vec<Value>,
    pub sql_count_query: String,
    pub expected_count: i64,
    pub id_str: &'static str,
    pub timeout: Duration,
}
