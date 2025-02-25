use std::io::Write;

use anyhow::Context;
use spacetimedb_paths::SpacetimePaths;

use super::install::{download_and_install, download_with_progress, make_progress_bar};

/// Upgrade and switch to the latest available version of SpacetimeDB.
#[derive(clap::Args)]
pub(super) struct Upgrade {}

impl Upgrade {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        super::tokio_block_on(async {
            let client = super::reqwest_client()?;
            let (version, release) = download_and_install(&client, None, None, paths).await?;
            paths.cli_bin_dir.set_current_version(&version.to_string())?;

            let cur_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
            if version > cur_version {
                if let Some(asset) = release.assets.iter().find(|asset| asset.name == UPDATE_BIN_NAME) {
                    let pb = make_progress_bar().with_prefix("Self-updating `spacetime version`: ");
                    pb.set_message("downloading...");
                    let bin = download_with_progress(&pb, &client, &asset.browser_download_url).await?;
                    pb.set_message("installing...");
                    let cli_bin_file = paths.cli_bin_file.clone();
                    tokio::task::spawn_blocking(move || {
                        // TODO(noa): try and see if `self_replace` could support providing the binary
                        // in a buffer, instead of an already existing file, since we're doing the same
                        // work they are right now
                        let mut file = tempfile::NamedTempFile::with_prefix_in(
                            ".spacetimedb-self-replace",
                            cli_bin_file.0.parent().unwrap(),
                        )?;
                        file.write_all(&bin.to_bytes())?;
                        self_replace::self_replace(file.path())
                            .context("failed to overwrite the original spacetime binary")
                    })
                    .await??;

                    pb.finish_with_message("done!");
                } else {
                    eprintln!("Tried to self-update `spacetime version`, but no release asset was found.");
                }
            }

            Ok(())
        })?
    }
}

const UPDATE_BIN_NAME: &str = if cfg!(windows) {
    concat!("spacetimedb-update-", env!("BUILD_TARGET"), ".exe")
} else {
    concat!("spacetimedb-update-", env!("BUILD_TARGET"))
};
