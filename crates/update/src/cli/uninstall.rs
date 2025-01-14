use spacetimedb_paths::SpacetimePaths;

use super::ForceYes;

/// Uninstall an installed SpacetimeDB version.
#[derive(clap::Args)]
pub(super) struct Uninstall {
    version: semver::Version,
    #[command(flatten)]
    yes: ForceYes,
}

impl Uninstall {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        let Self { version, yes } = self;
        if yes.confirm(format!("Uninstall v{version}?"))? {
            let dir = paths.cli_bin_dir.version_dir(&version);
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}
