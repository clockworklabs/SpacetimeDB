use spacetimedb_paths::SpacetimePaths;

/// Set the global default SpacetimeDB version.
#[derive(clap::Args)]
pub(super) struct Use {
    version: semver::Version,
}

impl Use {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        paths.cli_bin_dir.set_current_version(&self.version.to_string())
    }
}
