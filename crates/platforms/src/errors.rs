use crate::directories::PathArg;
use crate::metadata::EditionKind;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ErrorPlatform {
    #[error("IO: `{error}`, path: `{path}`")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("Fail to read Toml: `{error}`, path: `{path}`")]
    TomlDe { path: PathBuf, error: toml::de::Error },
    #[error("Fail to parse version: '{version}', should be en format 'major.minor.patch'")]
    ParseVersion { version: String },
    #[error("Edition Mismatch on `{path}`: expected '{expected:?}', found '{found:?}'")]
    EditionMismatch {
        path: PathBuf,
        expected: EditionKind,
        found: EditionKind,
    },
    #[error("Not a file: `{path}`")]
    NotFile { path: PathArg },
    #[error("Not a directory: `{path}`")]
    NotDirectory { path: PathArg },
    #[error("Root path is ambiguous: CLI root: `{root_cli}`, Config root: `{root_config}`")]
    RootMismatch { root_cli: PathBuf, root_config: PathBuf },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
