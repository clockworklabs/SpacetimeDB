use crate::utils::path_type;

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

path_type!(BinFile: file);
path_type!(BinDir: dir);

path_type!(CliTomlPath: file);
