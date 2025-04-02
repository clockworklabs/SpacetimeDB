use std::path::Path;
use std::{fmt, io};

use toml;
use toml_edit;

use spacetimedb_lib::ConnectionId;
use spacetimedb_paths::cli::{ConfigDir, PrivKeyPath, PubKeyPath};
use spacetimedb_paths::server::{ConfigToml, MetadataTomlPath};

/// Parse a TOML file at the given path, returning `None` if the file does not exist.
///
/// **WARNING**: Comments and formatting in the file will be lost.
pub fn parse_config<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(toml::from_str(&contents)?)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct MetadataFile {
    pub version: semver::Version,
    pub edition: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// Unused and always `None` in SpacetimeDB-standalone,
    /// but used by SpacetimeDB-cloud.
    pub client_connection_id: Option<ConnectionId>,
}

impl MetadataFile {
    pub fn new(edition: &str) -> Self {
        let mut current_version: semver::Version = env!("CARGO_PKG_VERSION").parse().unwrap();
        // set the patch version of newly-created metadata files to 0 -- v1.0.0
        // set `cmp.patch = Some(file_version.patch)` when checking version
        // compatibility, meaning it won't be forwards-compatible with a
        // database claiming to be created on v1.0.1, even though that should
        // work. This can be changed once we release v1.1.0, since we don't
        // care about its DBs being backwards-compatible with v1.0.0 anyway.
        if let semver::Version { major: 1, minor: 0, .. } = current_version {
            current_version.patch = 0;
        }
        Self {
            version: current_version,
            edition: edition.to_owned(),
            client_connection_id: None,
        }
    }

    pub fn read(path: &MetadataTomlPath) -> anyhow::Result<Option<Self>> {
        parse_config(path.as_ref())
    }

    pub fn write(&self, path: &MetadataTomlPath) -> io::Result<()> {
        path.write(self.to_string())
    }

    /// Check if this meta file is compatible with the default meta
    /// file of a just-started database, and if so return the metadata
    /// to write back to the file.
    ///
    /// `self` is the metadata file read from a database, and current is
    /// the default metadata file that the active database version would
    /// right to a new database.
    pub fn check_compatibility_and_update(mut self, current: Self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.edition == current.edition,
            "metadata.toml indicates that this database is from a different \
             edition of SpacetimeDB (running {:?}, but this database is {:?})",
            current.edition,
            self.edition,
        );
        let cmp = semver::Comparator {
            op: semver::Op::Caret,
            major: self.version.major,
            minor: Some(self.version.minor),
            patch: None,
            pre: self.version.pre.clone(),
        };
        anyhow::ensure!(
            cmp.matches(&current.version),
            "metadata.toml indicates that this database is from a newer, \
             incompatible version of SpacetimeDB (running {:?}, but this \
             database is from {:?})",
            current.version,
            self.version,
        );
        // bump the version in the file only if it's being run in a newer
        // database -- this won't do anything until we release v1.1.0, since we
        // set current.version.patch to 0 in Self::new() due to a bug in v1.0.0
        self.version = std::cmp::max(self.version, current.version);
        Ok(self)
    }
}

impl fmt::Display for MetadataFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# THIS FILE IS GENERATED BY SPACETIMEDB, DO NOT MODIFY!")?;
        writeln!(f)?;
        f.write_str(&toml::to_string(self).unwrap())
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigFile {
    #[serde(default)]
    pub certificate_authority: Option<CertificateAuthority>,
    #[serde(default)]
    pub logs: LogConfig,
}

impl ConfigFile {
    pub fn read(path: &ConfigToml) -> anyhow::Result<Option<Self>> {
        parse_config(path.as_ref())
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CertificateAuthority {
    pub jwt_priv_key_path: PrivKeyPath,
    pub jwt_pub_key_path: PubKeyPath,
}

impl CertificateAuthority {
    pub fn in_cli_config_dir(dir: &ConfigDir) -> Self {
        Self {
            jwt_priv_key_path: dir.jwt_priv_key(),
            jwt_pub_key_path: dir.jwt_pub_key(),
        }
    }

    pub fn get_or_create_keys(&self) -> anyhow::Result<crate::auth::JwtKeys> {
        crate::auth::get_or_create_keys(self)
    }
}

#[serde_with::serde_as]
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct LogConfig {
    #[serde_as(as = "Option<serde_with::DisplayFromStr>")]
    pub level: Option<tracing_core::LevelFilter>,
    #[serde(default)]
    pub directives: Vec<String>,
}

/// Update the value of a key in a `TOML` document, preserving the formatting and comments of the original value.
///
/// ie:
///
/// ```toml;no_run
/// # Moving key = value to key = new_value
/// old = "value" # Comment
/// new = "new_value" # Comment
/// ```
fn copy_value_with_decor(old_value: Option<&toml_edit::Item>, new_value: &str) -> toml_edit::Item {
    match old_value {
        Some(toml_edit::Item::Value(toml_edit::Value::String(old_value))) => {
            // Creates a new `toml_edit::Value` with the same formatting as the old value.
            let mut new = toml_edit::Value::String(toml_edit::Formatted::new(new_value.to_string()));
            let decor = new.decor_mut();
            // Copy the comments and formatting from the old value.
            *decor = old_value.decor().clone();
            new.into()
        }
        _ => new_value.into(),
    }
}

/// Set the value of a key in a `TOML` document, removing the key if the value is `None`.
///
/// **NOTE**: This function will preserve the formatting and comments of the original value.
pub fn set_opt_value(doc: &mut toml_edit::DocumentMut, key: &str, value: Option<&str>) {
    let old_value = doc.get(key);
    if let Some(new) = value {
        doc[key] = copy_value_with_decor(old_value, new);
    } else {
        doc.remove(key);
    }
}

/// Set the value of a key in a `TOML` table, removing the key if the value is `None`.
///
/// **NOTE**: This function will preserve the formatting and comments of the original value.
pub fn set_table_opt_value(table: &mut toml_edit::Table, key: &str, value: Option<&str>) {
    let old_value = table.get(key);
    if let Some(new) = value {
        table[key] = copy_value_with_decor(old_value, new);
    } else {
        table.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkver(major: u64, minor: u64, patch: u64) -> semver::Version {
        semver::Version::new(major, minor, patch)
    }

    fn mkmeta(major: u64, minor: u64, patch: u64) -> MetadataFile {
        MetadataFile {
            version: mkver(major, minor, patch),
            edition: "standalone".to_owned(),
            client_connection_id: None,
        }
    }

    #[test]
    fn check_metadata_compatibility_checking() {
        assert_eq!(
            mkmeta(1, 0, 0)
                .check_compatibility_and_update(mkmeta(1, 0, 1))
                .unwrap()
                .version,
            mkver(1, 0, 1)
        );
        assert_eq!(
            mkmeta(1, 0, 1)
                .check_compatibility_and_update(mkmeta(1, 0, 0))
                .unwrap()
                .version,
            mkver(1, 0, 1)
        );

        mkmeta(1, 1, 0)
            .check_compatibility_and_update(mkmeta(1, 0, 5))
            .unwrap_err();
        mkmeta(2, 0, 0)
            .check_compatibility_and_update(mkmeta(1, 3, 5))
            .unwrap_err();
    }
}
