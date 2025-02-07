use std::path::PathBuf;

use anyhow::Context;
use spacetimedb_paths::cli::BinDir;
use spacetimedb_paths::SpacetimePaths;

/// Set a local installation of SpacetimeDB as a custom version.
#[derive(clap::Args)]
pub(super) struct Link {
    /// The name of the custom installation, e.g. `dev`.
    name: String,
    /// The path to the directory with the SpacetimeDB binaries.
    path: PathBuf,

    /// Switch to this version after it's created.
    #[arg(long)]
    r#use: bool,
}

impl Link {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.name != BinDir::CURRENT_VERSION_DIR_NAME,
            "name cannot be `current`"
        );
        let mut path = std::env::current_dir()?;
        path.push(self.path);
        paths
            .cli_bin_dir
            .version_dir(&self.name)
            .create_custom(&path)
            .context("could not link custom version")?;
        if self.r#use {
            paths.cli_bin_dir.set_current_version(&self.name)?;
        }
        Ok(())
    }
}
