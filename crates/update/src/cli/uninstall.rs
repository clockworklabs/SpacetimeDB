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
        let dir = paths.cli_bin_dir.version_dir(&version);
        if !dir.0.exists() {
            anyhow::bail!("v{version} is not installed");
        }
        if yes.confirm(format!("Uninstall v{version}?"))? {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_paths::FromPathUnchecked;
    use spacetimedb_paths::RootDir;

    fn make_temp_paths() -> (tempfile::TempDir, SpacetimePaths) {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("spacetime");
        std::fs::create_dir_all(&base).unwrap();
        let root = RootDir::from_path_unchecked(base);
        let paths = SpacetimePaths::from_root_dir(&root);
        (tmp, paths)
    }

    #[test]
    fn test_uninstall_nonexistent_version_errors_before_prompt() {
        let (_tmp, paths) = make_temp_paths();
        let uninstall = Uninstall {
            version: "9.9.9".to_owned(),
            yes: ForceYes { yes: true },
        };
        let result = uninstall.exec(&paths);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("9.9.9"),
            "error should mention the version number"
        );
        assert!(
            err.to_string().contains("not installed"),
            "error should say 'not installed'"
        );
    }

    #[test]
    fn test_uninstall_current_version_errors() {
        let (_tmp, paths) = make_temp_paths();
        // Create the "current" symlink target so it exists on disk
        let current_dir = paths.cli_bin_dir.version_dir("2.0.0");
        std::fs::create_dir_all(&current_dir.0).unwrap();
        paths.cli_bin_dir.set_current_version("2.0.0").unwrap();

        let uninstall = Uninstall {
            version: "2.0.0".to_owned(),
            yes: ForceYes { yes: true },
        };
        let result = uninstall.exec(&paths);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("currently used version"),);
    }

    #[test]
    fn test_uninstall_current_keyword_errors() {
        let (_tmp, paths) = make_temp_paths();
        let uninstall = Uninstall {
            version: "current".to_owned(),
            yes: ForceYes { yes: true },
        };
        let result = uninstall.exec(&paths);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot remove `current`"),);
    }

    #[test]
    fn test_uninstall_existing_version_with_yes() {
        let (_tmp, paths) = make_temp_paths();
        let version_dir = paths.cli_bin_dir.version_dir("1.0.0");
        std::fs::create_dir_all(&version_dir.0).unwrap();
        // Create a dummy file so we can verify the directory existed
        std::fs::write(version_dir.0.join("spacetime"), "dummy").unwrap();

        assert!(version_dir.0.exists(), "version dir should exist before");

        let uninstall = Uninstall {
            version: "1.0.0".to_owned(),
            yes: ForceYes { yes: true },
        };
        uninstall.exec(&paths).unwrap();

        assert!(!version_dir.0.exists(), "version dir should be removed after uninstall");
    }
}
