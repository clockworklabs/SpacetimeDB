//! The spacetimedb directory structure, represented as a type hierarchy.
//!
//! # Directory Structure of the Database.
//!
//! [`SpacetimePaths`] holds the paths to the various directories used by the CLI & database.
//!
//! * **cli-bin-dir**: a directory under which all versions of all
//!   SpacetimeDB binaries is be stored. Each binary is stored in a
//!   directory named with version number of the binary in this directory. If a
//!   binary has any related files required by that binary which are specific to
//!   that version, for example, template configuration files, these files will be
//!   installed in this folder as well.
//!
//! * **cli-config-dir**: a directory where configuration and state for the CLI,
//!   as well as the keyfiles used by the server, are stored.
//!
//! * **cli-bin-file**: the location of the default spacetime CLI executable, which
//!   is a symlink to the actual `spacetime` binary in the cli-bin-dir.
//!
//! * **data-dir**: the directory where all persistent server & database files
//!   are stored.
//!
//! ## Unix Directory Structure
//!
//! On Unix-like platforms, such as Linux and macOS, the installation paths follow the
//! XDG conventions by default:
//!
//! * `cli-config-dir`: `$XDG_CONFIG_HOME/spacetime/`
//! * `cli-bin-dir`: `$XDG_DATA_HOME/spacetime/bin/`
//! * `cli-bin-file`: `$XDG_BIN_HOME/spacetime`
//! * `data-dir`: `$XDG_DATA_HOME/spacetime/data`
//!
//! As per the XDG base directory specification, those base directories fall back to
//! to the following defaults if the corresponding environment variable is not set:
//!
//! * `$XDG_CONFIG_HOME`: `$HOME/.config`
//! * `$XDG_DATA_HOME`: `$HOME/.local/share`
//! * `$XDG_BIN_HOME`: `$HOME/.local/bin`
//!
//! For reference, the below is an example installation using the default paths:
//!
//!```sh
//! $HOME
//! ├── .local
//! │   ├── bin
//! │   │   └── spacetime -> $HOME/.local/share/spacetime/bin/1.10.1/spacetimedb-update # Current, in $PATH
//! │   └── share
//! │       └── spacetime
//! │           ├── bin
//! │           |   └── 1.10.1
//! │           |       ├── spacetimedb-update # Version manager
//! │           |       ├── spacetimedb-cli # CLI
//! │           |       ├── spacetimedb-standalone # Server
//! │           |       ├── spacetimedb-cloud # Server
//! │           |       ├── cli.default.toml # Template CLI configuration file
//! │           |       └── config.default.toml # Template server configuration file
//! |           └── data/
//! |
//! └── .config
//!     └── spacetime
//!         ├── id_ecdsa # Private key
//!         ├── id_ecdsa.pub # Public key
//!         └── cli.toml # CLI configuration
//! ```
//!
//!## Windows Directory Structure
//!
//! On Windows the installation paths follow Windows conventions, and is equivalent
//! to a Root Directory (as defined below) at `%LocalAppData%\SpacetimeDB\`.
//!
//! > **Note**: the `SpacetimeDB` directory is in `%LocalAppData%` and not `%AppData%`.
//! > This is intentional so that different users on Windows can have different
//! > configuration and binaries. This also allows you to install SpacetimeDB on Windows
//! > even if you are not a privileged user.
//!
//! ## Custom Root Directory
//!
//! Users on all platforms must be allowed to override the default installation
//! paths entirely with a single `--root-dir` argument passed to the initial
//! installation commands.
//!
//! If users specify a `--root-dir` flag, then the installation paths should be
//! defined relative to the `root-dir` as follows:
//!
//! * `cli-config-dir`: `{root-dir}/config/`
//! * `cli-bin-dir`: `{root-dir}/bin/`
//! * `cli-bin-file`: `{root-dir}/spacetime[.exe]`
//! * `data-dir`: `{root-dir}/data/`
//!
//! For reference, the below is an example installation using the `--root-dir` argument:
//!
//! ```sh
//! {root-dir}
//! ├── spacetime -> {root-dir}/bin/1.10.1/spacetimedb-update # Current, in $PATH
//! ├── config
//! │   ├── id_ecdsa # Private key
//! │   ├── id_ecdsa.pub # Public key
//! │   └── cli.toml # CLI configuration
//! ├── bin
//! |   └── 1.10.1
//! |       ├── spacetimedb-update.exe # Version manager
//! |       ├── spacetimedb-cli.exe # CLI
//! |       ├── spacetimedb-standalone.exe # Server
//! |       ├── spacetimedb-cloud.exe # Server
//! |       ├── cli.default.toml # Template CLI configuration file
//! |       └── config.default.toml # Template server configuration file
//! └── data/
//! ```
//!
//! # Data directory structure
//!
//! The following is an example of the internal structure of data-dir. Note that this is not
//! a stable hierarchy, and users should not rely on it being stable from version to version.
//!
//! ```sh
//! {data-dir} # {Data}: CLI (--data-dir)
//! ├── spacetime.pid # Lock file to prevent multiple instances, should be set to the pid of the running instance
//! ├── config.toml # Server configuration (Human written, machine read only)
//! ├── metadata.toml # Contains the edition, the MAJOR.MINOR.PATCH version number of the SpacetimeDB executable that created this directory. (Human readable, machine written only)
//! ├── program-bytes # STANDALONE ONLY! Wasm modules aka `ProgramStorage` /var/lib/spacetime/data/standalone/2/program_bytes (NOTE: renamed from program_bytes)
//! │   └── d6
//! │       └── d9e66a8a285a416abd87e847c48b0990c6db6a5e0d5670c79a13f75dcabbe6
//! ├── control-db # STANDALONE ONLY! Store information about the SpacetimeDB instances (NOTE: renamed from control_db)
//! │   ├── blobs/ # Blobs storage
//! │   ├── conf # Configuration for the Sled database
//! │   └── db # Sled database
//! ├── cache
//! │   └── wasmtime
//! ├── logs
//! │   └── spacetimedb-standalone.2024-07-08.log  # filename format: `spacetimedb-{edition}.YYYY-MM-DD.log`
//! └── replicas
//!     ├── 1 # Database `replica_id`, unique per cluster
//!     │   ├── clog # `CommitLog` files
//!     │   │   └── 00000000000000000000.stdb.log
//!     │   ├── module_logs # Module logs
//!     │   │   └── 2024-07-08.log # filename format: `YYYY-MM-DD.log`
//!     │   └── snapshots # Snapshots of the database
//!     │       └── 00000000000000000000.snapshot_dir # BSATN-encoded `Snapshot`
//!     │           ├── 00000000000000000000.snapshot_bsatn
//!     │           └── objects # Objects storage
//!     │               └── 01
//!     │                   └── 040a8585e6dc2c579c0c8f6017c7e6a0179a5d0410cd8db4b4affbd7d4d04f
//!     └── 34 # Database `replica_id`, unique per cluster
//!         ├── clog # `CommitLog` files
//!         │   └── 00000000000000000000.stdb.log
//!         ├── module_logs # Module logs
//!         │   └── 2024-07-08.log # filename format: `YYYY-MM-DD.log`
//!         └── snapshots # Snapshots of the database
//!             └── 00000000000000000000.snapshot_dir # BSATN-encoded `Snapshot`
//!                 ├── 00000000000000000000.snapshot_bsatn
//!                 └── objects # Objects storage directory trie
//!                     └── 01
//!                         └── 040a8585e6dc2c579c0c8f6017c7e6a0179a5d0410cd8db4b4affbd7d4d04f
//! ```

use crate::utils::PathBufExt;

pub mod cli;
pub mod server;
pub mod standalone;
mod utils;

#[doc(hidden)]
pub use serde as __serde;

/// Implemented for path types. Use `from_path_unchecked()` to construct a strongly-typed
/// path directly from a `PathBuf`.
pub trait FromPathUnchecked {
    /// The responsibility is on the caller to verify that the path is valid
    /// for this directory structure node.
    fn from_path_unchecked(path: impl Into<std::path::PathBuf>) -> Self;
}

path_type! {
    /// The --root-dir for the spacetime installation, if specified.
    // TODO: replace cfg(any()) with cfg(false) once stabilized
    #[non_exhaustive(any())]
    RootDir
}

impl RootDir {
    pub fn cli_config_dir(&self) -> cli::ConfigDir {
        cli::ConfigDir(self.0.join("config"))
    }

    pub fn cli_bin_file(&self) -> cli::BinFile {
        cli::BinFile(self.0.join("spacetime").with_exe_ext())
    }

    pub fn cli_bin_dir(&self) -> cli::BinDir {
        cli::BinDir(self.0.join("bin"))
    }

    pub fn data_dir(&self) -> server::ServerDataDir {
        server::ServerDataDir(self.0.join("data"))
    }

    fn from_paths(paths: &SpacetimePaths) -> Option<Self> {
        let SpacetimePaths {
            cli_config_dir,
            cli_bin_file,
            cli_bin_dir,
            data_dir,
        } = paths;
        let parent = cli_config_dir.0.parent()?;
        let parents = [cli_bin_file.0.parent()?, cli_bin_dir.0.parent()?, data_dir.0.parent()?];
        parents.iter().all(|x| *x == parent).then(|| Self(parent.to_owned()))
    }
}

#[derive(Clone, Debug)]
pub struct SpacetimePaths {
    pub cli_config_dir: cli::ConfigDir,
    pub cli_bin_file: cli::BinFile,
    pub cli_bin_dir: cli::BinDir,
    pub data_dir: server::ServerDataDir,
}

impl SpacetimePaths {
    /// Get the default directories for the currrent platform.
    ///
    /// Returns an error if the platform director(y/ies) cannot be found.
    pub fn platform_defaults() -> anyhow::Result<Self> {
        #[cfg(windows)]
        {
            let data_dir = dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("Could not find LocalAppData"))?;
            let root_dir = RootDir(data_dir.joined("SpacetimeDB"));
            Ok(Self::from_root_dir(&root_dir))
        }
        #[cfg(not(windows))]
        {
            // `dirs` doesn't use XDG base dirs on macOS, which we want to do,
            // so we use the `xdg` crate instead.
            let base_dirs = xdg::BaseDirectories::with_prefix("spacetime")?;
            // bin_home should really be in the xdg crate
            let xdg_bin_home = std::env::var_os("XDG_BIN_HOME")
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| {
                    #[allow(deprecated)] // this is fine on non-windows platforms
                    std::env::home_dir().unwrap().joined(".local/bin")
                });

            let exe_name = "spacetime";

            Ok(Self {
                cli_config_dir: cli::ConfigDir(base_dirs.get_config_home()),
                cli_bin_file: cli::BinFile(xdg_bin_home.join(exe_name)),
                cli_bin_dir: cli::BinDir(base_dirs.get_data_file("bin")),
                data_dir: server::ServerDataDir(base_dirs.get_data_file("data")),
            })
        }
    }

    pub fn from_root_dir(dir: &RootDir) -> Self {
        Self {
            cli_config_dir: dir.cli_config_dir(),
            cli_bin_file: dir.cli_bin_file(),
            cli_bin_dir: dir.cli_bin_dir(),
            data_dir: dir.data_dir(),
        }
    }

    pub fn to_root_dir(&self) -> Option<RootDir> {
        RootDir::from_paths(self)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use super::*;

    mod vars {
        use super::*;
        struct ResetVar<'a>(&'a str, Option<OsString>);
        impl Drop for ResetVar<'_> {
            fn drop(&mut self) {
                maybe_set_var(self.0, self.1.as_deref())
            }
        }
        fn maybe_set_var(var: &str, val: Option<impl AsRef<OsStr>>) {
            if let Some(val) = val {
                std::env::set_var(var, val);
            } else {
                std::env::remove_var(var);
            }
        }
        pub(super) fn with_vars<const N: usize, R>(vars: [(&str, Option<&str>); N], f: impl FnOnce() -> R) -> R {
            let _guard = vars.map(|(var, val)| {
                let prev_val = std::env::var_os(var);
                maybe_set_var(var, val);
                ResetVar(var, prev_val)
            });
            f()
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn xdg() {
        let p = Path::new;
        let paths = vars::with_vars(
            [
                ("XDG_CONFIG_HOME", Some("/__config_home")),
                ("XDG_DATA_HOME", Some("/__data_home")),
                ("XDG_BIN_HOME", Some("/__bin_home")),
            ],
            SpacetimePaths::platform_defaults,
        )
        .unwrap();
        assert_eq!(paths.cli_config_dir.0, p("/__config_home/spacetime"));
        assert_eq!(paths.cli_bin_file.0, p("/__bin_home/spacetime"));
        assert_eq!(paths.cli_bin_dir.0, p("/__data_home/spacetime/bin"));
        assert_eq!(paths.data_dir.0, p("/__data_home/spacetime/data"));
    }

    #[cfg(windows)]
    #[test]
    fn windows() {
        let paths = SpacetimePaths::platform_defaults().unwrap();
        let appdata_local = dirs::data_local_dir().unwrap();
        assert_eq!(paths.cli_config_dir.0, appdata_local.join("config"));
        assert_eq!(paths.cli_bin_file.0, appdata_local.join("spacetime.exe"));
        assert_eq!(paths.cli_bin_dir.0, appdata_local.join("bin"));
        assert_eq!(paths.data_dir.0, appdata_local.join("data"));
    }

    #[test]
    fn custom() {
        let root = Path::new("/custom/path");
        let paths = SpacetimePaths::from_root_dir(&RootDir(root.to_owned()));
        assert_eq!(paths.cli_config_dir.0, root.join("config"));
        assert_eq!(paths.cli_bin_file.0, root.join("spacetime").with_exe_ext());
        assert_eq!(paths.cli_bin_dir.0, root.join("bin"));
        assert_eq!(paths.data_dir.0, root.join("data"));
    }
}
