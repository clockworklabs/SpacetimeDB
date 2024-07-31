use std::path::PathBuf;
use std::{fmt, fs};

use crate::directories::{BaseDirectories, PathArg, Source};
use crate::errors::ErrorPlatform;
use crate::metadata::{Bin, ConfigPaths, Edition, EditionKind, Metadata, Version};
use crate::toml::read_toml_if_exists;

pub const CONFIG_CLIENT: &str = "client.toml";
pub const CONFIG_SERVER: &str = "server.toml";

/// Configuration files.
#[derive(Clone)]
pub struct Config {
    pub(crate) path: PathBuf,
    pub(crate) client: PathBuf,
    pub(crate) server: PathBuf,
}

impl Config {
    pub fn new(path: PathBuf) -> Self {
        Self {
            client: path.join(CONFIG_CLIENT),
            server: path.join(CONFIG_SERVER),
            path,
        }
    }
    /// Returns the path of the client config file `client.toml`.
    pub fn client_file(&self) -> &PathBuf {
        &self.client
    }
    /// Returns the path of the server config file `server.toml`.
    pub fn server_file(&self) -> &PathBuf {
        &self.server
    }
    /// Returns the path of the logs config file `log.conf`.
    pub fn log_file(&self) -> PathBuf {
        self.path.join("log.conf")
    }
    /// Returns the path of the public key file `id_ecdsa.pub`.
    pub fn public_key_file(&self) -> PathBuf {
        self.path.join("id_ecdsa.pub")
    }
    /// Returns the path of the private key file `id_ecdsa`.
    pub fn private_key_file(&self) -> PathBuf {
        self.path.join("id_ecdsa")
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("path", &self.path)
            .field("client_file", &self.client_file())
            .field("server_file", &self.server_file())
            .field("log_file", &self.log_file())
            .field("public_key_file", &self.public_key_file())
            .field("private_key_file", &self.private_key_file())
            .finish()
    }
}

/// An installation with its `path` and `version`.
#[derive(Clone)]
pub struct Install {
    path: PathBuf,
    version: Version,
}

impl Install {
    /// Returns the `path` of the update manager binary.
    pub fn manager_file(&self) -> PathBuf {
        self.path.join(Bin::Update.name())
    }

    /// Returns the `path` of the spacetime binary.
    pub fn spacetime_file(&self) -> PathBuf {
        self.path.join(Bin::Spacetime.name())
    }

    /// Returns the `path` of the standalone binary.
    pub fn standalone_file(&self) -> PathBuf {
        self.path.join(Bin::StandAlone.name())
    }

    /// Returns the `path` of the cloud binary.
    pub fn cloud_file(&self) -> PathBuf {
        self.path.join(Bin::Cloud.name())
    }

    /// Returns the `path` of the CLI binary.
    pub fn cli_file(&self) -> PathBuf {
        self.path.join(Bin::Cli.name())
    }
}

impl fmt::Debug for Install {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Install")
            .field("path", &self.path)
            .field("version", &self.version)
            .field("manager_file", &self.manager_file())
            .field("spacetime_file", &self.spacetime_file())
            .field("standalone_file", &self.standalone_file())
            .field("cloud_file", &self.cloud_file())
            .field("cli_file", &self.cli_file())
            .finish()
    }
}

/// Collection of installed `binaries`.
#[derive(Debug)]
pub struct Installed {
    pub installed: Vec<Install>,
}

/// Cache paths.
#[derive(Clone)]
pub struct Cache {
    pub path: PathBuf,
}

impl Cache {
    /// Returns the path of the Wasmtime directory within the cache.
    pub fn wasmtime_dir(&self) -> PathBuf {
        self.path.join("wasmtime")
    }
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cache")
            .field("path", &self.path)
            .field("wasmtime_dir", &self.wasmtime_dir())
            .finish()
    }
}

/// Log paths.
#[derive(Clone)]
pub struct Logs {
    pub path: PathBuf,
    edition: Edition,
}

impl Logs {
    /// Constructs a log file path for a given binary and path.
    fn log_name(&self, path: PathBuf, of: Bin) -> PathBuf {
        path.join(format!("{}-{}.log", of.name(), self.edition.version.to_filename()))
    }
    /// Returns the `path` of the `module logs` directory.
    pub fn module_dir(&self) -> PathBuf {
        self.path.join("module_logs")
    }
    /// Returns the `path` of the `module log` file for a given binary.
    pub fn module_file(&self, of: Bin) -> PathBuf {
        self.log_name(self.module_dir(), of)
    }
    /// Returns the path of the `program log` file for a given binary.
    pub fn program_file(&self, of: Bin) -> PathBuf {
        self.log_name(self.path.clone(), of)
    }
}

impl fmt::Debug for Logs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Logs")
            .field("path", &self.path)
            .field("module_dir", &self.module_dir())
            .finish()
    }
}

/// `Path` to a specific spacetime instance.
pub struct Instance {
    pub path: PathBuf,
    pub instance_id: u64,
}

impl Instance {
    /// Returns the `path` of the `clog` directory.
    pub fn clog_dir(&self) -> PathBuf {
        self.path.join("clog")
    }
    /// Returns the `path` of the `scheduler` directory.
    pub fn scheduler_dir(&self) -> PathBuf {
        self.path.join("scheduler")
    }
    /// Returns the `path` of the `snapshots` directory.
    pub fn snapshots_dir(&self) -> PathBuf {
        self.path.join("snapshots")
    }
}

impl fmt::Debug for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance")
            .field("path", &self.path)
            .field("instance_id", &self.instance_id)
            .field("clog_dir", &self.clog_dir())
            .field("scheduler_dir", &self.scheduler_dir())
            .field("snapshots_dir", &self.snapshots_dir())
            .finish()
    }
}

/// Struct representing data paths.
#[derive(Clone)]
pub struct Data {
    pub path: PathBuf,
    pub cache: Cache,
}

impl Data {
    pub fn new(path: PathBuf) -> Self {
        Self {
            cache: Cache {
                path: path.join("cache"),
            },
            path,
        }
    }
    /// Returns the path of the PID file.
    pub fn pid_file(&self) -> PathBuf {
        self.path.join("spacetime.pid")
    }
    /// Returns the path of the metadata file.
    pub fn metadata_file(&self) -> PathBuf {
        self.path.join("metadata.toml")
    }
    /// Returns the path of the program bytes standalone directory.
    ///
    /// NOTE: Is only used for the _standalone edition_.
    pub fn program_bytes_standalone_dir(&self) -> PathBuf {
        self.path.join("program-bytes")
    }
    /// Returns the path of the control database standalone directory.
    ///
    /// NOTE: Is only used for the _standalone edition_.
    pub fn control_db_standalone_dir(&self) -> PathBuf {
        self.path.join("control-db")
    }
    pub fn instances_dir(&self) -> PathBuf {
        self.path.join("instances")
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Data")
            .field("path", &self.path)
            .field("cache", &self.cache)
            .field("pid_file", &self.pid_file())
            .field("metadata_file", &self.metadata_file())
            .field("program_bytes_standalone_dir", &self.program_bytes_standalone_dir())
            .field("control_db_standalone_dir", &self.control_db_standalone_dir())
            .field("instances_dir", &self.instances_dir())
            .finish()
    }
}

/// The `root` path for the default [Data] & [Install] directories.
#[derive(Clone)]
pub struct Var {
    pub path: PathBuf,
}

impl Var {
    /// Returns the path of the *install* directory.
    pub fn install_dir(&self) -> PathBuf {
        self.path.join("bin")
    }
}

impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Var")
            .field("path", &self.path)
            .field("install_dir", &self.install_dir())
            .finish()
    }
}

/// Generate the paths used by Spacetime.
///
/// NOTE: It only calculates the paths, not verify or create them.
#[derive(Clone)]
pub struct SpacetimePaths {
    pub edition: Edition,
    pub root: Option<PathBuf>,
    pub bin_dir: PathBuf,
    pub config: Config,
    pub data: Data,
    pub log: Logs,
    pub var: Var,
}

impl SpacetimePaths {
    pub fn new(edition: Edition, dirs: BaseDirectories) -> Self {
        let data = dirs.data_dir();
        Self {
            edition,
            root: dirs.root.path,
            bin_dir: dirs.bin.path,
            config: Config::new(dirs.config.path),
            data: Data::new(data),
            log: Logs {
                path: dirs.var.path.join("logs"),
                edition,
            },
            var: Var { path: dirs.var.path },
        }
    }
}

#[derive(Debug, Clone)]
pub struct FsOptions {
    edition: Edition,
    root: Option<PathBuf>,
    config: Option<PathBuf>,
    config_client: Option<PathBuf>,
    config_server: Option<PathBuf>,
    data: Option<PathBuf>,
    use_logs: bool,
}

impl FsOptions {
    pub fn standalone(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            edition: Edition::standalone(major, minor, patch),
            root: None,
            config: None,
            config_client: None,
            config_server: None,
            data: None,
            use_logs: false,
        }
    }

    pub fn cloud(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            edition: Edition::cloud(major, minor, patch),
            root: None,
            config: None,
            config_client: None,
            config_server: None,
            data: None,
            use_logs: false,
        }
    }
    pub fn root(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }
    pub fn config(mut self, config: PathBuf) -> Self {
        self.config = Some(config);
        self
    }

    pub fn config_client(mut self, config_client: PathBuf) -> Self {
        self.config_client = Some(config_client);
        self
    }

    pub fn config_server(mut self, config_server: PathBuf) -> Self {
        self.config_server = Some(config_server);
        self
    }

    pub fn data(mut self, data: PathBuf) -> Self {
        self.data = Some(data);
        self
    }

    pub fn use_logs(mut self, use_logs: bool) -> Self {
        self.use_logs = use_logs;
        self
    }
}

#[derive(Debug, Clone)]
pub struct SpacetimeFs {
    paths: SpacetimePaths,
    use_logs: bool,
}

impl SpacetimeFs {
    pub fn resolve(options: FsOptions) -> Result<BaseDirectories, ErrorPlatform> {
        let FsOptions {
            edition: _,
            root,
            config,
            config_client,
            config_server,
            data,
            use_logs: _,
        } = options;

        let mut dirs = BaseDirectories::platform();

        if let Some(config) = config {
            dirs = dirs.with_config_dir(Source::Cli, config);

            if let Some(config) = read_toml_if_exists::<ConfigPaths, _>(&dirs.config.path)? {
                if let (Some(root_cli), Some(root_config)) = (dirs.root.path.as_ref(), config.paths.root.as_ref()) {
                    if root_cli != root_config {
                        return Err(ErrorPlatform::RootMismatch {
                            root_cli: root_cli.clone(),
                            root_config: root_config.clone(),
                        });
                    }
                }

                if let Some(path) = config.paths.root {
                    dirs = dirs.with_root(Source::Config, path);
                }

                if let Some(path) = config.paths.data {
                    dirs = dirs.with_data(Source::Config, path);
                }
                if let Some(path) = config.paths.config_server {
                    dirs = dirs.with_data(Source::Config, path);
                }
            }
        }

        if let Some(config_client) = config_client {
            dirs = dirs.with_config_dir(Source::Cli, config_client);
        }
        if let Some(config_server) = config_server {
            dirs = dirs.with_config_dir(Source::Cli, config_server);
        }
        if let Some(root) = root {
            dirs = dirs.with_root(Source::Cli, root);
        }
        if let Some(data) = data {
            dirs = dirs.with_data(Source::Cli, data);
        }
        Ok(dirs)
    }

    pub fn new(options: FsOptions) -> Result<Self, ErrorPlatform> {
        let edition = options.edition;
        let use_logs = options.use_logs;
        let dirs = Self::resolve(options)?;

        Self::verify_layout(edition, &dirs)?;

        Ok(Self {
            paths: SpacetimePaths::new(edition, dirs),
            use_logs,
        })
    }

    pub fn create(options: FsOptions) -> Result<Self, ErrorPlatform> {
        let edition = options.edition;
        let use_logs = options.use_logs;
        let dirs = Self::resolve(options)?;

        fn create_dir(path: &PathBuf) -> Result<(), ErrorPlatform> {
            fs::create_dir_all(path).map_err(|error| ErrorPlatform::IO {
                path: path.clone(),
                error,
            })
        }

        if let Some(root) = dirs.root.path.as_ref() {
            create_dir(root)?;
        }
        create_dir(&dirs.config.path)?;
        create_dir(&dirs.bin.path)?;
        create_dir(&dirs.var.path)?;
        // TODO: Create the files

        if let Some(root) = dirs.data.path.as_ref() {
            create_dir(root)?;
        }

        let paths = SpacetimePaths::new(edition, dirs);
        create_dir(&paths.data.cache.path)?;
        create_dir(&paths.data.instances_dir())?;

        if edition.kind == EditionKind::StandAlone {
            create_dir(&paths.data.program_bytes_standalone_dir())?;
            create_dir(&paths.data.control_db_standalone_dir())?;
        }

        Ok(Self { paths, use_logs })
    }

    fn verify_layout(edition: Edition, dirs: &BaseDirectories) -> Result<(), ErrorPlatform> {
        dirs.root.is_dir()?;
        dirs.bin.is_dir()?;
        dirs.var.is_dir()?;
        dirs.config.is_dir()?;
        dirs.data.is_dir()?;

        let data = Data::new(dirs.data_dir());

        let check_path = |path: PathBuf| -> Result<(), ErrorPlatform> {
            let path = PathArg {
                path,
                source: dirs.data.source,
            };
            path.is_dir()
        };

        if edition.kind == EditionKind::StandAlone {
            check_path(data.program_bytes_standalone_dir())?;
            check_path(data.control_db_standalone_dir())?;
        }

        check_path(data.instances_dir())?;
        check_path(data.cache.path)?;

        let is_server = if dirs.config_server.exists() {
            dirs.config_server.is_file()?;
            true
        } else {
            false
        };
        if dirs.config_client.exists() {
            dirs.config_client.is_file()?;
        }

        if is_server {
            let data = Data::new(dirs.data_dir());
            if data.metadata_file().exists() {
                let metadata = Metadata::from_path(data.metadata_file())?;
                if metadata.edition.kind != edition.kind {
                    return Err(ErrorPlatform::EditionMismatch {
                        path: data.path,
                        expected: edition.kind,
                        found: metadata.edition.kind,
                    });
                }
            }
        }

        Ok(())
    }
}

impl fmt::Debug for SpacetimePaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpacetimePaths")
            .field("edition", &self.edition)
            .field("root", &self.root)
            .field("bin_dir", &self.bin_dir)
            .field("config", &self.config)
            .field("data", &self.data)
            .field("log", &self.log)
            .field("var", &self.var)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    const LAYOUT: &str = r#"SpacetimePaths {
    edition: Edition {
        kind: StandAlone,
        version: Version {
            major: 0,
            minor: 1,
            patch: 0,
        },
    },
    root: Some(
        "/root",
    ),
    bin_dir: "/root/bin",
    config: Config {
        path: "/root/config",
        client_file: "/root/config/client.toml",
        server_file: "/root/config/server.toml",
        log_file: "/root/config/log.conf",
        public_key_file: "/root/config/id_ecdsa.pub",
        private_key_file: "/root/config/id_ecdsa",
    },
    data: Data {
        path: "/root/var/data",
        cache: Cache {
            path: "/root/var/data/cache",
            wasmtime_dir: "/root/var/data/cache/wasmtime",
        },
        pid_file: "/root/var/data/spacetime.pid",
        metadata_file: "/root/var/data/metadata.toml",
        program_bytes_standalone_dir: "/root/var/data/program-bytes",
        control_db_standalone_dir: "/root/var/data/control-db",
        instances_dir: "/root/var/data/instances",
    },
    log: Logs {
        path: "/root/var/logs",
        module_dir: "/root/var/logs/module_logs",
    },
    var: Var {
        path: "/root/var",
        install_dir: "/root/var/bin",
    },
}"#;

    #[test]
    fn correct_layout() {
        let dirs = BaseDirectories::custom(PathBuf::from("/root"));
        let fs = SpacetimePaths::new(Edition::standalone(0, 1, 0), dirs);

        assert_eq!(format!("{:#?}", fs), LAYOUT);
    }

    #[test]
    fn fs() {
        let tmp = tempfile::tempdir().unwrap();

        let fs = SpacetimeFs::create(FsOptions::standalone(0, 1, 0).root(tmp.into_path())).unwrap();

        assert!(fs.paths.root.unwrap().exists());
        assert!(fs.paths.bin_dir.exists());
        assert!(fs.paths.data.cache.path.exists());
        assert!(fs.paths.data.path.exists());
        // TODO: Create the files
        // assert!(fs.paths.config.client_file().exists());
        // assert!(fs.paths.config.server_file().exists());
        // assert!(fs.paths.config.log_file().exists());
        // assert!(fs.paths.config.public_key_file().exists());
        // assert!(fs.paths.config.private_key_file().exists());
    }
}
