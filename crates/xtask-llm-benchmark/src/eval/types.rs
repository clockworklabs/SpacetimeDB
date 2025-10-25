use serde::{Deserialize, Serialize};

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
