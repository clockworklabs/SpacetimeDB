use spacetimedb_paths::SpacetimePaths;

/// Upgrade and switch to the latest available version of SpacetimeDB.
#[derive(clap::Args)]
pub(super) struct Upgrade {}

impl Upgrade {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        super::tokio_block_on(async {
            let client = super::reqwest_client()?;
            let version = super::install::download_and_install(&client, None, None, paths).await?;
            paths.cli_bin_dir.set_current_version(&version)?;

            let cur_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
            if version > cur_version {
                let mut new_update_binary = paths.cli_bin_dir.version_dir(&version).0.join("spacetimedb-update");
                new_update_binary.set_extension(std::env::consts::EXE_EXTENSION);
                if new_update_binary.exists() {
                    tokio::fs::copy(new_update_binary, &paths.cli_bin_file).await?;
                    eprintln!("Self-updated `spacetime version` command")
                }
            }

            Ok(())
        })?
    }
}
