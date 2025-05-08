use spacetimedb_paths::SpacetimePaths;
use std::path::Path;

/// List installed SpacetimeDB versions.
#[derive(clap::Args)]
pub(super) struct List {
    /// List all versions available to download and install.
    #[arg(long)]
    all: bool,
}

impl List {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        let current = match paths.cli_bin_dir.current_version()? {
            None => None,
            Some(path_str) => {
                let file_name = Path::new(&path_str)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .ok_or(anyhow::anyhow!("Could not extract current version"))?;
                Some(file_name.to_string())
            }
        };
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
