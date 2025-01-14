use anyhow::Context;

use crate::utils::{path_type, PathBufExt};

path_type! {
    /// The configuration directory for the CLI & keyfiles.
    ConfigDir: dir
}

impl ConfigDir {
    pub fn jwt_priv_key(&self) -> PrivKeyPath {
        PrivKeyPath(self.0.join("id_ecdsa"))
    }
    pub fn jwt_pub_key(&self) -> PubKeyPath {
        PubKeyPath(self.0.join("id_ecdsa.pub"))
    }
    pub fn cli_toml(&self) -> CliTomlPath {
        CliTomlPath(self.0.join("cli.toml"))
    }
}

// TODO: replace cfg(any()) with cfg(false) once stabilized
path_type!(#[non_exhaustive(any())] PrivKeyPath: file);
path_type!(#[non_exhaustive(any())] PubKeyPath: file);

path_type!(CliTomlPath: file);

path_type!(BinFile: file);

path_type!(BinDir: dir);

impl BinDir {
    pub fn version_dir(&self, version: &semver::Version) -> VersionBinDir {
        VersionBinDir(self.0.join(version.to_string()))
    }

    pub fn current_version_dir(&self) -> VersionBinDir {
        VersionBinDir(self.0.join("current"))
    }

    pub fn set_current_version(&self, version: &semver::Version) -> anyhow::Result<()> {
        let link_path = self.current_version_dir();
        #[cfg(unix)]
        {
            // remove the link if it already exists
            std::fs::remove_file(&link_path).ok();
            std::os::unix::fs::symlink(version.to_string(), link_path)?;
        }
        #[cfg(windows)]
        {
            junction::delete(&link_path).ok();
            let version_path = self.version_dir(version);
            // We won't be able to create a junction if the fs isn't NTFS, so fall back to trying
            // to make a symlink.
            junction::create(&version_path, &link_path)
                .or_else(|err| std::os::windows::fs::symlink_dir(version.to_string(), &link_path).or(Err(err)))?;
        }
        Ok(())
    }

    pub fn current_version(&self) -> anyhow::Result<Option<semver::Version>> {
        match std::fs::read_link(self.current_version_dir()) {
            Ok(path) => path
                .to_str()
                .context("not utf8")
                .and_then(|s| s.parse::<semver::Version>().map_err(Into::into))
                .context("could not parse `current` symlink as a version number")
                .map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn installed_versions(&self) -> anyhow::Result<Vec<semver::Version>> {
        self.read_dir()?
            .filter_map(|r| match r {
                Ok(entry) => {
                    let parsed: semver::Version = entry.file_name().to_str()?.parse().ok()?;
                    Some(anyhow::Ok(parsed))
                }
                Err(e) => Some(Err(e.into())),
            })
            .collect()
    }
}

path_type!(VersionBinDir: dir);

impl VersionBinDir {
    pub fn spacetimedb_cli(self) -> SpacetimedbCliBin {
        SpacetimedbCliBin(self.0.joined("spacetimedb-cli").with_exe_ext())
    }
}

path_type!(SpacetimedbCliBin: file);
