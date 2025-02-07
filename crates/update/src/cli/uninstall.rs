use anyhow::Context;
use spacetimedb_paths::cli::BinDir;
use spacetimedb_paths::SpacetimePaths;

use super::ForceYes;

/// Uninstall an installed SpacetimeDB version.
#[derive(clap::Args)]
pub(super) struct Uninstall {
    version: String,
    #[command(flatten)]
    yes: ForceYes,
}

impl Uninstall {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        let Self { version, yes } = self;
        anyhow::ensure!(
            version != BinDir::CURRENT_VERSION_DIR_NAME,
            "cannot remove `current` version"
        );
        match paths
            .cli_bin_dir
            .current_version()
            .context("couldn't read current version")
        {
            Ok(Some(current)) => anyhow::ensure!(version != current, "cannot uninstall currently used version"),
            Ok(None) => {}
            Err(e) => tracing::warn!("{e:#}"),
        }
        if yes.confirm(format!("Uninstall v{version}?"))? {
            let dir = paths.cli_bin_dir.version_dir(&version);
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}
