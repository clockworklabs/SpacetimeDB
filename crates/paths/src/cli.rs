use std::path::Path;

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
    pub fn version_dir(&self, version: &str) -> VersionBinDir {
        VersionBinDir(self.0.join(version))
    }

    pub const CURRENT_VERSION_DIR_NAME: &str = "current";
    pub fn current_version_dir(&self) -> VersionBinDir {
        VersionBinDir(self.0.join(Self::CURRENT_VERSION_DIR_NAME))
    }

    pub fn set_current_version(&self, version: &str) -> anyhow::Result<()> {
        self.current_version_dir().link_to(self.version_dir(version).as_ref())
    }

    pub fn current_version(&self) -> anyhow::Result<Option<String>> {
        match std::fs::read_link(self.current_version_dir()) {
            Ok(path) => path.into_os_string().into_string().ok().context("not utf8").map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn installed_versions(&self) -> anyhow::Result<Vec<String>> {
        self.read_dir()?
            .filter_map(|r| match r {
                Ok(entry) => {
                    let name = entry.file_name();
                    if name == Self::CURRENT_VERSION_DIR_NAME {
                        None
                    } else {
                        entry.file_name().into_string().ok().map(Ok)
                    }
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

    pub fn create_custom(&self, path: &Path) -> anyhow::Result<()> {
        if std::fs::symlink_metadata(self).is_ok_and(|m| m.file_type().is_dir()) {
            anyhow::bail!("version already exists");
        }
        self.link_to(path)
    }

    fn link_to(&self, path: &Path) -> anyhow::Result<()> {
        let rel_path = path.strip_prefix(self).unwrap_or(path);
        #[cfg(unix)]
        {
            // remove the link if it already exists
            std::fs::remove_file(self).ok();
            std::os::unix::fs::symlink(rel_path, self)?;
        }
        #[cfg(windows)]
        {
            junction::delete(self).ok();
            // We won't be able to create a junction if the fs isn't NTFS, so fall back to trying
            // to make a symlink.
            junction::create(path, self)
                .or_else(|err| std::os::windows::fs::symlink_dir(rel_path, self).or(Err(err)))?;
        }
        Ok(())
    }
}

path_type!(SpacetimedbCliBin: file);
