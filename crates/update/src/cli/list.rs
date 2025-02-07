use spacetimedb_paths::SpacetimePaths;

/// List installed SpacetimeDB versions.
#[derive(clap::Args)]
pub(super) struct List {
    /// List all versions available to download and install.
    #[arg(long)]
    all: bool,
}

impl List {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        let current = paths.cli_bin_dir.current_version()?;
        let versions = if self.all {
            let client = super::reqwest_client()?;
            super::tokio_block_on(super::install::available_releases(&client))??
        } else {
            paths.cli_bin_dir.installed_versions()?
        };
        for ver in versions {
            print!("{ver}");
            if Some(&ver) == current.as_ref() {
                print!(" (current)");
            }
            println!();
        }
        Ok(())
    }
}
