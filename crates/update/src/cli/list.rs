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
        // This `match` part is only here because at one point we had a bug where we were creating
        // symlinks that contained the _entire_ path, rather than just the relative path to the
        // version directory. It's not strictly necessary, it just fixes our determination of what
        // the current version is, for the output of this command.
        //
        // That symlink bug was fixed in `crates/paths/src/cli.rs` in
        // https://github.com/clockworklabs/SpacetimeDB/pull/2680, but this `match` means that this
        // output will still be correct for any users that already have one of the bugged symlinks.
        //
        // Once users upgrade to a version containing #2680, they will have the code that creates
        // the fixed symlinks. However, that code won't immediately run, since the upgrade will be
        // running from the previous binary they had. So once they upgrade to a version containing
        // #2680, _and then_ upgrade once more, their symlinks will be fixed. There's no real
        // timeline on when everyone will have done that, but hopefully that helps give a sense of
        // how long this code "should" exist for (but it doesn't do any harm afaik).
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
