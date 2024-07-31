use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::errors::ErrorPlatform;
use crate::toml::read_toml;
use serde::{Deserialize, Serialize};
use spacetimedb_lib::Address;

/// Enum representing different binaries.
pub enum Bin {
    Spacetime,
    StandAlone,
    Cloud,
    Cli,
    Update,
}

impl Bin {
    pub fn name(&self) -> &'static str {
        match self {
            Bin::Spacetime => "spacetime",
            Bin::StandAlone => "spacetimedb-standalone",
            Bin::Cloud => "spacetimedb-cloud",
            Bin::Cli => "spacetimedb-cli",
            Bin::Update => "spacetimedb-up",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self { major, minor, patch }
    }

    pub fn to_filename(&self) -> String {
        format!("{}_{}_{}", self.major, self.minor, self.patch)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = ErrorPlatform;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        let parse_u64 = |s: &str| -> Result<u64, ErrorPlatform> {
            s.parse()
                .map_err(|_| ErrorPlatform::ParseVersion { version: s.to_string() })
        };

        match parts[..] {
            [major, minor, patch] => {
                let major = parse_u64(major)?;
                let minor = parse_u64(minor)?;
                let patch = parse_u64(patch)?;
                Ok(Version::new(major, minor, patch))
            }
            _ => Err(ErrorPlatform::ParseVersion { version: s.to_string() }),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum EditionKind {
    StandAlone,
    Cloud,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Edition {
    pub(crate) kind: EditionKind,
    pub(crate) version: Version,
}

impl Edition {
    pub fn cloud(major: u64, minor: u64, patch: u64) -> Self {
        Edition {
            kind: EditionKind::Cloud,
            version: Version::new(major, minor, patch),
        }
    }

    pub fn standalone(major: u64, minor: u64, patch: u64) -> Self {
        Edition {
            kind: EditionKind::StandAlone,
            version: Version::new(major, minor, patch),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPath {
    pub root: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub config_server: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPaths {
    pub paths: ConfigPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawMetadata {
    pub edition: Edition,
    pub client_address: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub edition: Edition,
    pub client_address: Option<Address>,
}

impl Metadata {
    pub fn from_path(path: PathBuf) -> Result<Self, ErrorPlatform> {
        let config: RawMetadata = read_toml(path)?;

        let client_address = if let Some(x) = config.client_address.as_deref() {
            Some(Address::from_hex(x)?)
        } else {
            None
        };
        Ok(Metadata {
            edition: config.edition,
            client_address,
        })
    }
}
