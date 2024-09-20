use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::errors::ErrorPlatform;

pub fn read_toml<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<T, ErrorPlatform> {
    let path = path.as_ref();

    let content = std::fs::read_to_string(path).map_err(|error| ErrorPlatform::IO {
        path: path.to_path_buf(),
        error,
    })?;

    toml::from_str(&content).map_err(|error| ErrorPlatform::TomlDe {
        error,
        path: path.to_path_buf(),
    })
}

pub fn write_toml<T: Serialize, P: AsRef<Path>>(path: P, value: &T, comment: &str) -> Result<(), ErrorPlatform> {
    let path = path.as_ref();
    let mut content = String::new();
    if !comment.is_empty() {
        content.push_str(&format!("# {}\n\n", comment));
    }
    let toml_content = toml::to_string(value).map_err(|error| ErrorPlatform::TomlSer {
        path: path.to_path_buf(),
        error,
    })?;
    content.push_str(&toml_content);

    std::fs::write(path, content).map_err(|error| ErrorPlatform::IO {
        path: path.to_path_buf(),
        error,
    })
}
