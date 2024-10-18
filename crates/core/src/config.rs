use std::env::temp_dir;
use std::fmt;
use std::path::{Path, PathBuf};

use spacetimedb_lib::Address;
use spacetimedb_paths::server::MetadataTomlPath;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod paths {
    use super::*;

    /// The default path for the database files.
    pub(super) fn db_path() -> PathBuf {
        PathBuf::from("/stdb")
    }

    /// The default path for the database logs.
    pub(super) fn logs_path() -> PathBuf {
        PathBuf::from("/var/log")
    }

    /// The default path for the database config files.
    pub(super) fn config_path() -> PathBuf {
        PathBuf::from("/etc/spacetimedb/")
    }
}

#[cfg(target_os = "macos")]
mod paths {
    use super::*;

    /// The default path for the database files.
    pub(super) fn db_path() -> PathBuf {
        PathBuf::from("/usr/local/var/stdb")
    }

    /// The default path for the database logs.
    pub(super) fn logs_path() -> PathBuf {
        PathBuf::from("/var/log")
    }

    /// The default path for the database config files.
    pub(super) fn config_path() -> PathBuf {
        PathBuf::from("/etc/spacetimedb/")
    }
}

#[cfg(target_os = "windows")]
mod paths {
    use super::*;

    /// The default path for the database files.
    pub(super) fn db_path() -> PathBuf {
        dirs::data_dir()
            .map(|x| x.join("stdb"))
            .expect("failed to read the windows `data directory`")
    }

    /// The default path for the database logs.
    pub(super) fn logs_path() -> PathBuf {
        db_path().join("log")
    }

    /// The default path for the database config files.
    pub(super) fn config_path() -> PathBuf {
        dirs::config_dir()
            .map(|x| x.join("stdb"))
            .expect("Fail to read the windows `config directory`")
    }
}

/// Returns the default path for the database in the `OS` temporary directory.
pub fn stdb_path_temp() -> PathBuf {
    temp_dir().join("stdb")
}

/// Types specifying where to find various files needed by spacetimedb.
pub trait SpacetimeDbFiles {
    /// The path for the database files.
    fn db_path(&self) -> PathBuf;

    /// The path for the database logs.
    fn logs(&self) -> PathBuf;

    /// The path for the database config files.
    fn config(&self) -> PathBuf;

    /// The path of the database config file `log.conf` for logs.
    fn log_config(&self) -> PathBuf {
        self.config().join("log.conf")
    }

    /// The path of the private key file `id_ecdsa`.
    fn private_key(&self) -> PathBuf {
        self.config().join("id_ecdsa")
    }

    /// The path of the public key file `id_ecdsa.pub`.
    fn public_key(&self) -> PathBuf {
        self.config().join("id_ecdsa.pub")
    }
}

/// The location of paths for the database in a local OR temp folder.
pub struct FilesLocal {
    dir: PathBuf,
}

impl FilesLocal {
    /// Create a new [FilesLocal], appending `name` to the `temp` folder returned by [stdb_path_temp].
    pub fn temp(name: &str) -> Self {
        assert!(!name.is_empty(), "`name` should be filled");

        Self {
            dir: stdb_path_temp().join(name),
        }
    }

    /// Create a new [FilesLocal] that is in a hidden `path + .spacetime` folder.
    pub fn hidden<P: AsRef<Path>>(path: P) -> Self {
        Self {
            dir: path.as_ref().join(".spacetime"),
        }
    }
}

impl SpacetimeDbFiles for FilesLocal {
    fn db_path(&self) -> PathBuf {
        self.dir.clone()
    }

    fn logs(&self) -> PathBuf {
        self.db_path().join("logs")
    }

    fn config(&self) -> PathBuf {
        self.db_path().join("conf")
    }
}

/// The global location of paths for the database.
///
/// NOTE: This location varies by OS.
pub struct FilesGlobal;

impl SpacetimeDbFiles for FilesGlobal {
    fn db_path(&self) -> PathBuf {
        paths::db_path()
    }

    fn logs(&self) -> PathBuf {
        paths::logs_path()
    }

    fn config(&self) -> PathBuf {
        paths::config_path()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct MetadataFile {
    pub version: semver::Version,
    pub edition: String,
    pub client_address: Option<Address>,
}

impl MetadataFile {
    pub fn read(path: &MetadataTomlPath) -> anyhow::Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(contents) => Ok(Some(toml::from_str(&contents)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn write(&self, path: &MetadataTomlPath) -> std::io::Result<()> {
        std::fs::write(path, self.to_string())
    }

    pub fn version_compatible_with(&self, version: &semver::Version) -> bool {
        semver::Comparator {
            op: semver::Op::Caret,
            major: self.version.major,
            minor: Some(self.version.minor),
            patch: Some(self.version.patch),
            pre: self.version.pre.clone(),
        }
        .matches(version)
    }
}

impl fmt::Display for MetadataFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# THIS FILE IS GENERATED BY SPACETIMEDB, DO NOT MODIFY!")?;
        writeln!(f)?;
        f.write_str(&toml::to_string(self).unwrap())
    }
}

pub fn current_version() -> semver::Version {
    env!("CARGO_PKG_VERSION").parse().unwrap()
}
