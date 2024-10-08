use std::path::PathBuf;

use thiserror::Error;

use crate::metadata::{EditionKind, Version};

#[derive(Error, Debug)]
pub enum ErrorPlatform {
    #[error("IO: `{error}`, path: `{path}`")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("Glob: `{error}`, path: `{path}`")]
    Glob { path: PathBuf, error: glob::PatternError },
    #[error("Fail to read Toml: `{error}`, path: `{path}`")]
    TomlDe { path: PathBuf, error: toml::de::Error },
    #[error("Fail to write Toml: `{error}`, path: `{path}`")]
    TomlSer { path: PathBuf, error: toml::ser::Error },
    #[error("Fail to parse version: '{version}', should be en format 'major.minor.patch'")]
    ParseVersion { version: String },
    #[error("Edition Mismatch on `{path}`: expected '{expected:?}', found '{found:?}'")]
    EditionMismatch {
        path: PathBuf,
        expected: EditionKind,
        found: EditionKind,
    },
    #[error("Version Mismatch on `{path}`: expected '{expected:?}', found '{found:?}'")]
    VersionMismatch {
        path: PathBuf,
        expected: Version,
        found: Version,
    },
    #[error("Not a file: `{path}`")]
    NotFile { path: PathBuf },
    #[error("Not a directory: `{path}`")]
    NotDirectory { path: PathBuf },
    #[error("Root path is ambiguous: CLI root: `{root_cli}`, Config root: `{root_config}`")]
    RootMismatch { root_cli: PathBuf, root_config: PathBuf },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
