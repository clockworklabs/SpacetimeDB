use crate::directories::{Directories, PathArg, Source};
use crate::errors::ErrorPlatform;
use crate::metadata::{Bin, ConfigPaths, Edition, EditionKind, Metadata, Version};
use crate::toml::read_toml_if_exists;
use spacetimedb_lib::Address;
use std::path::PathBuf;
use std::{fmt, fs};

/// Configuration files.
#[derive(Clone)]
pub struct Config {
    /// Path to the configuration directory.
    pub(crate) path: PathBuf,
    /// Path to the client configuration file, default: "client.toml".
    pub(crate) client: PathBuf,
    /// Path to the server configuration file, default: "server.toml".
    pub(crate) server: PathBuf,
}

impl Config {
    /// Create a new configuration with the given `path`.
    ///
    /// The `client` and `server` configuration files are set to `client.toml` and `server.toml` respectively.
    pub fn new(path: PathBuf) -> Self {
        Self {
            client: path.join("client.toml"),
            server: path.join("server.toml"),
            path,
        }
    }
    /// Returns the path of the client config file.
    pub fn client_file(&self) -> &PathBuf {
        &self.client
    }
    /// Returns the path of the server config file .
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

/// An installation directory.
#[derive(Clone)]
pub struct Install {
    /// Path to the installation directory.
    path: PathBuf,
    /// Version of the installed binaries.
    version: Version,
}

impl Install {
    /// Returns the `path` of the  [Bin::Update] binary.
    pub fn manager_file(&self) -> PathBuf {
        self.path.join(Bin::Update.name())
    }

    /// Returns the `path` of the [Bin::Spacetime] binary.
    pub fn spacetime_file(&self) -> PathBuf {
        self.path.join(Bin::Spacetime.name())
    }

    /// Returns the `path` of the [Bin::StandAlone] binary.
    pub fn standalone_file(&self) -> PathBuf {
        self.path.join(Bin::StandAlone.name())
    }

    /// Returns the `path` of the [Bin::Cloud] binary.
    pub fn cloud_file(&self) -> PathBuf {
        self.path.join(Bin::Cloud.name())
    }

    /// Returns the `path` of the [Bin::Cli] binary.
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
    /// Path to the cache directory.
    pub path: PathBuf,
}

impl Cache {
    /// Returns the `path` for the `Wasmtime` directory cache.
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
    /// Path to the `logs` directory.
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
    /// Returns the `path` of a `module log` file for a given binary.
    pub fn module_file(&self, of: Bin) -> PathBuf {
        self.log_name(self.module_dir(), of)
    }
    /// Returns the path of a `program log` file for a given binary.
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
    /// Path to the instance directory.
    pub path: PathBuf,
    /// The `instance id`.
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
    /// Path to the `data` directory.
    pub path: PathBuf,
    /// [Cache] paths.
    pub cache: Cache,
}

impl Data {
    /// Constructs a new `data` path with the given `path`.
    ///
    /// The `cache` directory is set to `path/cache`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            cache: Cache {
                path: path.join("cache"),
            },
            path,
        }
    }
    /// Returns the path of the `PID` file.
    pub fn pid_file(&self) -> PathBuf {
        self.path.join("spacetime.pid")
    }
    /// Returns the path of the `metadata` file.
    pub fn metadata_file(&self) -> PathBuf {
        self.path.join("metadata.toml")
    }
    /// Returns the path of the `program bytes` standalone directory.
    ///
    /// NOTE: Should be used for the _standalone edition_ only.
    pub fn program_bytes_standalone_dir(&self) -> PathBuf {
        self.path.join("program-bytes")
    }
    /// Returns the path of the `control database` standalone directory.
    ///
    /// NOTE: Should be used for the _standalone edition_ only.
    pub fn control_db_standalone_dir(&self) -> PathBuf {
        self.path.join("control-db")
    }
    /// Returns the path of the `instances` directory.
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
/// WARNING: It only *calculates* the paths, not verify or create them.
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
    /// Create a new instance of `SpacetimePaths` from the [Directories].
    pub fn new(edition: Edition, dirs: Directories) -> Self {
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

/// Options for creating a new [SpacetimeFs].
#[derive(Debug, Clone)]
pub struct FsOptions {
    edition: Edition,
    /// Change the root directory.
    root: Option<PathBuf>,
    /// Change the configuration directory.
    config: Option<PathBuf>,
    /// Change the client configuration file.
    config_client: Option<PathBuf>,
    /// Change the server configuration file.
    config_server: Option<PathBuf>,
    /// Change the data directory.
    data: Option<PathBuf>,
    /// If `true`, create the log directories.
    use_logs: bool,
    /// If is [EditionKind::Cloud], the client address.
    address: Option<Address>,
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
            address: None,
        }
    }

    pub fn cloud(major: u64, minor: u64, patch: u64, address: Address) -> Self {
        Self {
            edition: Edition::cloud(major, minor, patch),
            root: None,
            config: None,
            config_client: None,
            config_server: None,
            data: None,
            use_logs: false,
            address: Some(address),
        }
    }

    /// Change the `root` directory.
    pub fn root(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }

    /// Change the configuration directory.
    pub fn config(mut self, config: PathBuf) -> Self {
        self.config = Some(config);
        self
    }

    /// Change the client configuration file.
    pub fn config_client(mut self, config_client: PathBuf) -> Self {
        self.config_client = Some(config_client);
        self
    }

    /// Change the server configuration file.
    pub fn config_server(mut self, config_server: PathBuf) -> Self {
        self.config_server = Some(config_server);
        self
    }

    /// Change the `data` directory.
    pub fn data(mut self, data: PathBuf) -> Self {
        self.data = Some(data);
        self
    }

    /// If `true`, create the `logs` directories.
    pub fn use_logs(mut self, use_logs: bool) -> Self {
        self.use_logs = use_logs;
        self
    }
}

/// Spacetime file system.
///
/// It provides the paths used by Spacetime, and assert that the layout is correct.
#[derive(Debug, Clone)]
pub struct SpacetimeFs {
    paths: SpacetimePaths,
    use_logs: bool,
}

impl SpacetimeFs {
    /// Resolve the directories & files from the given [FsOptions].
    ///
    /// * Directories: `root`, `data`, `config`
    /// * Files: `config_client`, `config_server`
    ///
    /// Resolution order:
    ///
    ///  The `Locations` are resolved as follows, _merging the values_, from top to bottom, so we exhaust all the possibilities
    ///  using this priority:
    ///
    ///  1. Resolve locally relative to the binary's folder, look for a `Linux` layout folder structure, with the
    ///     prefix `.spacetime`.
    ///  2. Otherwise:
    ///     * On `Linux`/`macOS`, use the `XDG` convention, with the prefix `spacetime`.
    ///     * On `Windows`, install to local user on `%LocalAppData%`, with the prefix `SpacetimeDB`.
    ///  3. Read the `{Config}/client.toml`, if it exists and contains the `Locations`, else maintain the current
    ///     structure.
    ///  4. Explicitly set by `cli` parameters.
    ///
    /// **NOTE:** If the `root` is set by the `config` or the `cli`, the `linux` convention is used.
    ///
    /// **ERROR:** If the `root` is set by the `cli` and the `config`, and they are different, it will return [ErrorPlatform::RootMismatch].
    pub fn resolve(options: FsOptions) -> Result<Directories, ErrorPlatform> {
        let FsOptions {
            edition: _,
            root,
            config,
            config_client,
            config_server,
            data,
            use_logs: _,
            address: _,
        } = options;

        let local = Directories::current_dir()?;

        let mut dirs = if local.root.is_dir().is_ok()
            && local.config.path.exists()
            && local.var.path.exists()
            && local.bin.path.exists()
        {
            local
        } else {
            Directories::platform()
        };

        if let Some(config) = config {
            dirs = dirs.config_dir(Source::Cli, config);
        }
        if let Some(config_client) = config_client {
            dirs = dirs.config_client_file(Source::Cli, config_client);
        }

        if let Some(config_client) = dirs.config_client.path.as_ref() {
            if let Some(config) = read_toml_if_exists::<ConfigPaths, _>(config_client)? {
                if let (Some(root_cli), Some(root_config)) = (dirs.root.path.as_ref(), config.root.as_ref()) {
                    if root_cli != root_config {
                        return Err(ErrorPlatform::RootMismatch {
                            root_cli: root_cli.clone(),
                            root_config: root_config.clone(),
                        });
                    }
                }
                if let Some(path) = config.root {
                    dirs = dirs.root(Source::Config, path);
                }
                if let Some(path) = config.data {
                    dirs = dirs.data(Source::Config, path);
                }
                if let Some(path) = config.config_server {
                    dirs = dirs.config_server_file(Source::Config, path);
                }
            }
        }

        if let Some(config_server) = config_server {
            dirs = dirs.config_server_file(Source::Cli, config_server);
        }
        if let Some(root) = root {
            dirs = dirs.root(Source::Cli, root);
        }
        if let Some(data) = data {
            dirs = dirs.data(Source::Cli, data);
        }

        Ok(dirs)
    }

    /// Open an existing spacetime file system.
    ///
    /// It [Self::verify_layout] of the directories, but not create them.
    pub fn open(options: FsOptions) -> Result<Self, ErrorPlatform> {
        let edition = options.edition;
        let use_logs = options.use_logs;
        let dirs = Self::resolve(options)?;

        Self::verify_layout(edition, &dirs)?;

        Ok(Self {
            paths: SpacetimePaths::new(edition, dirs),
            use_logs,
        })
    }

    /// Create a new spacetime file system.
    ///
    /// It creates the directories and files.
    pub fn create(options: FsOptions) -> Result<Self, ErrorPlatform> {
        let edition = options.edition;
        let use_logs = options.use_logs;
        let client_address = options.address;

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

        let data = Data::new(dirs.data_dir());
        create_dir(&data.path)?;

        let meta = Metadata {
            edition,
            client_address,
        };
        meta.write(data.metadata_file())?;

        let paths = SpacetimePaths::new(edition, dirs);
        create_dir(&paths.data.cache.path)?;
        create_dir(&paths.data.instances_dir())?;

        if edition.kind == EditionKind::StandAlone {
            create_dir(&paths.data.program_bytes_standalone_dir())?;
            create_dir(&paths.data.control_db_standalone_dir())?;
        }

        if use_logs {
            create_dir(&paths.log.path)?;
            create_dir(&paths.log.module_dir())?;
        }

        Ok(Self { paths, use_logs })
    }

    /// Verify the layout of the directories.
    ///
    /// It checks if the directories and files are in the correct layout, and the paths are valid (with `is_dir` and `is_file`).
    ///
    /// **ERROR**: If the `metadata` file exists and the  recorded `edition` is different from the given, it will return [ErrorPlatform::EditionMismatch].
    fn verify_layout(edition: Edition, dirs: &Directories) -> Result<(), ErrorPlatform> {
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
                let metadata = Metadata::read(data.metadata_file())?;
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
        let dirs = Directories::custom(PathBuf::from("/root"));
        let fs = SpacetimePaths::new(Edition::standalone(0, 1, 0), dirs);

        assert_eq!(format!("{:#?}", fs), LAYOUT);
    }

    #[test]
    fn fs_create() {
        let tmp = tempfile::tempdir().unwrap();

        let options = FsOptions::standalone(0, 1, 0).root(tmp.into_path());

        let fs = SpacetimeFs::create(options.clone()).unwrap();

        assert!(fs.paths.root.unwrap().exists());
        assert!(fs.paths.bin_dir.exists());
        assert!(fs.paths.data.cache.path.exists());
        assert!(fs.paths.data.path.exists());
        assert!(!fs.paths.log.path.exists());

        assert!(fs.paths.data.metadata_file().exists());
        // TODO: Create the files
        // assert!(fs.paths.config.client_file().exists());
        // assert!(fs.paths.config.server_file().exists());
        // assert!(fs.paths.config.log_file().exists());
        // assert!(fs.paths.config.public_key_file().exists());
        // assert!(fs.paths.config.private_key_file().exists());

        let fs = SpacetimeFs::create(options.use_logs(true)).unwrap();
        assert!(fs.paths.log.path.exists());
    }

    #[test]
    fn fs_open() {
        let tmp = tempfile::tempdir().unwrap();

        let options = FsOptions::standalone(0, 1, 0).root(tmp.into_path());

        SpacetimeFs::create(options.clone()).unwrap();

        let fs = SpacetimeFs::open(options).unwrap();

        assert!(fs.paths.root.unwrap().exists());
        assert!(fs.paths.bin_dir.exists());
        assert!(fs.paths.data.cache.path.exists());
        assert!(fs.paths.data.path.exists());
        assert!(!fs.paths.log.path.exists());
    }
}
