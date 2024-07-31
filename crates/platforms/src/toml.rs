use crate::errors::ErrorPlatform;
use serde::de::DeserializeOwned;
use std::path::Path;

fn _read_toml<T: DeserializeOwned>(content: &str, path: &Path) -> Result<T, ErrorPlatform> {
    toml::from_str(content).map_err(|error| ErrorPlatform::TomlDe {
        error,
        path: path.to_path_buf(),
    })
}

pub fn read_toml<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<T, ErrorPlatform> {
    let path = path.as_ref();

    let content = std::fs::read_to_string(path).map_err(|error| ErrorPlatform::IO {
        path: path.to_path_buf(),
        error,
    })?;

    _read_toml(&content, path)
}

pub fn read_toml_if_exists<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<Option<T>, ErrorPlatform> {
    let path = path.as_ref();

    if let Ok(content) = std::fs::read_to_string(path) {
        Ok(Some(_read_toml(&content, path)?))
    } else {
        Ok(None)
    }
}
