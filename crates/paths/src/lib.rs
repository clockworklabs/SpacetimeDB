//! The spacetimedb directory tructure, represented as a type hierarchy.

use crate::utils::PathBufExt;

pub mod cli;
pub mod server;
pub mod standalone;
mod utils;

#[doc(hidden)]
pub use serde as __serde;

path_type! {
    /// The --root-dir for the spacetime installation, if specified.
    RootDir: dir
}

impl RootDir {
    pub fn cli_config_dir(&self) -> cli::ConfigDir {
        cli::ConfigDir(self.0.join("config"))
    }

    pub fn cli_bin_file(&self) -> cli::BinFile {
        let mut path = self.0.join("spacetime");
        path.set_extension(std::env::consts::EXE_EXTENSION);
        cli::BinFile(path)
    }

    pub fn cli_bin_dir(&self) -> cli::BinDir {
        cli::BinDir(self.0.join("bin"))
    }

    pub fn data_dir(&self) -> server::ServerDataDir {
        server::ServerDataDir(self.0.join("data"))
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
            let base_dirs = xdg::BaseDirectories::with_prefix("spacetime")?;
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
}
