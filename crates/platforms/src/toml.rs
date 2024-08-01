use crate::errors::ErrorPlatform;
use serde::de::DeserializeOwned;
use serde::Serialize;
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

pub fn write_toml<T: Serialize, P: AsRef<Path>>(path: P, value: &T) -> Result<(), ErrorPlatform> {
    let path = path.as_ref();
    let content = toml::to_string(value).map_err(|error| ErrorPlatform::Other(error.into()))?;
    std::fs::write(path, content).map_err(|error| ErrorPlatform::IO {
        path: path.to_path_buf(),
        error,
    })
}
pub fn read_toml_if_exists<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<Option<T>, ErrorPlatform> {
    let path = path.as_ref();

    if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|error| ErrorPlatform::IO {
            path: path.to_path_buf(),
            error,
        })?;
        Ok(Some(_read_toml(&content, path)?))
    } else {
        Ok(None)
    }
}
