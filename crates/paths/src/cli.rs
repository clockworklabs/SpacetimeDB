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

path_type!(#[non_exhaustive(FALSE)] PrivKeyPath: file);
path_type!(#[non_exhaustive(FALSE)] PubKeyPath: file);

path_type!(BinFile: file);

path_type!(BinDir: dir);

impl BinDir {
    pub fn version_dir(&self, version: semver::Version) -> VersionBinDir {
        VersionBinDir(self.0.join(version.to_string()))
    }
}

path_type!(VersionBinDir: dir);

impl VersionBinDir {
    pub fn spacetimedb_cli(self) -> SpacetimedbCliBin {
        SpacetimedbCliBin(self.0.joined("spacetimedb-cli").with_exe_ext())
    }
}

path_type!(SpacetimedbCliBin: file);

path_type!(CliTomlPath: file);
