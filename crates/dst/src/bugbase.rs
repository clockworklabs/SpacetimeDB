//! Generic persistence helpers for failure artifacts.

use std::{fs, path::Path};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Generic persisted failure artifact for one deterministic run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BugArtifact<C, F> {
    pub seed: u64,
    pub failure: F,
    pub case: C,
    pub shrunk_case: Option<C>,
}

/// Writes any serializable value to disk as pretty JSON.
pub fn save_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, body)?;
    Ok(())
}

/// Loads any JSON value written by [`save_json`].
pub fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> anyhow::Result<T> {
    let body = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}
