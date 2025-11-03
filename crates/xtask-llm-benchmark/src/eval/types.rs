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
    pub notes: serde_json::Value,
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
