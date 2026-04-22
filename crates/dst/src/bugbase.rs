//! Generic persistence helpers for failure artifacts.

use std::{fs, path::Path};

use serde::{de::DeserializeOwned, Serialize};

/// Writes any serializable value to disk as pretty JSON.
pub(crate) fn save_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, body)?;
    Ok(())
}

/// Loads any JSON value written by [`save_json`].
pub(crate) fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> anyhow::Result<T> {
    let body = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}
