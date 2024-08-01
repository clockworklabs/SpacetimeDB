use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fmt, fs};

use crate::directories::{CheckPath, Directories, Layout};
use crate::errors::ErrorPlatform;
use crate::metadata::{Bin, EditionKind, EditionOptions, Metadata, Version};
use spacetimedb_lib::Address;

/// Recursively create a directory and all of its parent components, if they
/// are missing.
fn create_dir(path: &PathBuf) -> Result<(), ErrorPlatform> {
    fs::create_dir_all(path).map_err(|error| ErrorPlatform::IO {
        path: path.clone(),
        error,
    })
}

/// Returns an iterator over the entries within a directory.
pub fn read_dir(path: &Path) -> impl Iterator<Item = Result<PathBuf, ErrorPlatform>> + '_ {
    fs::read_dir(path)
        .map_err(|error| ErrorPlatform::IO {
            path: path.into(),
            error,
        })
        .into_iter()
        .flatten()
        .map(|entry| {
            entry.map(|e| e.path()).map_err(|error| ErrorPlatform::IO {
                path: path.into(),
                error,
            })
        })
}

/// Filter the entries of a directory by a [glob::Pattern].
pub fn glob_path<'a>(path: &'a Path, pattern: &'a str) -> impl Iterator<Item = Result<PathBuf, ErrorPlatform>> + 'a {
    let pattern = glob::Pattern::new(&format!("{}/{pattern}", path.display()))
        .map_err(|error| ErrorPlatform::Glob {
            path: path.into(),
            error,
        })
        .unwrap();

    read_dir(path).filter(move |entry| match entry {
        Ok(entry) => pattern.matches_path(entry),
        Err(_) => true,
    })
}

/// File system operations.
pub trait Fs {
    /// Verify the layout of the file system.
    ///
    /// Check the structure of the directories and files, but not create them.
    fn verify_layout(&self) -> Result<(), ErrorPlatform>;
    /// Create the layout of the file system.
    ///
    /// Create the directories and the files that are part of the layout (only)
    ///
    /// **NOTE:** It doesn't create the files that are part of the data, like logs, replicas, etc.
    fn create_layout(&self) -> Result<(), ErrorPlatform>;
    /// Load the layout of the file system.
    ///
    /// Load the directories and the files that are part of the layout (only)
    ///
    /// **NOTE:** It doesn't load the files that are part of the data, like logs, replicas, etc.
    fn load_layout(&mut self) -> Result<(), ErrorPlatform>;
}

/// A configuration directory.
#[derive(Clone)]
pub struct Config {
    /// Path to the configuration directory.
    pub path: PathBuf,
}

impl Config {
    /// Create a new configuration with the given `path`.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    /// Returns the path of the `cli.toml` config file.
    pub fn cli_file(&self) -> PathBuf {
        self.path.join("cli.toml")
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
            .field("client_file", &self.cli_file())
            .field("public_key_file", &self.public_key_file())
            .field("private_key_file", &self.private_key_file())
            .finish()
    }
}

/// An installation directory.
#[derive(Clone)]
pub struct Install {
    layout: Layout,
    /// Path to the installation directory.
    pub path: PathBuf,
    /// Version of the installed binaries.
    pub version: Version,
}

impl Install {
    /// Returns the `path` of the [Bin::Cli] binary.
    pub fn cli_file(&self) -> PathBuf {
        self.path.join(Bin::Cli.name(self.layout))
    }

    /// Returns the `path` of the [Bin::Cloud] binary.
    pub fn cloud_file(&self) -> PathBuf {
        self.path.join(Bin::Cloud.name(self.layout))
    }

    /// Returns the `path` of the  [Bin::Update] binary.
    pub fn update_file(&self) -> PathBuf {
        self.path.join(Bin::Update.name(self.layout))
    }

    /// Returns the `path` of the [Bin::StandAlone] binary.
    pub fn standalone_file(&self) -> PathBuf {
        self.path.join(Bin::StandAlone.name(self.layout))
    }

    /// Returns the `path` of the template CLI configuration file.
    pub fn cli_template_file(&self) -> PathBuf {
        self.path.join("cli.default.toml")
    }

    /// Returns the `path` of the template server configuration file.
    pub fn config_template_file(&self) -> PathBuf {
        self.path.join("config.default.toml")
    }
}

impl fmt::Debug for Install {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Install")
            .field("path", &self.path)
            .field("version", &self.version)
            .field("cli_file", &self.cli_file())
            .field("cloud_file", &self.cloud_file())
            .field("update_file", &self.update_file())
            .field("standalone_file", &self.standalone_file())
            .field("cli_template_file", &self.cli_template_file())
            .field("config_template_file", &self.config_template_file())
            .finish()
    }
}

/// Collection of installed `binaries`.
#[derive(Clone, Default)]
pub struct Installed {
    // We want to keep the binaries sorted by version.
    pub versions: BTreeMap<Version, Install>,
}

impl fmt::Debug for Installed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installed")
            .field("versions", &self.versions.values())
            .finish()
    }
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

/// Binaries paths.
#[derive(Clone, Debug)]
pub struct Binaries {
    layout: Layout,
    /// Path to the `bin` directory.
    pub path: PathBuf,
    pub installed: Installed,
}

impl Binaries {
    pub fn new(path: PathBuf, layout: Layout) -> Self {
        Self {
            layout,
            path,
            installed: Installed::default(),
        }
    }

    /// Add a new installation with the given [Version].
    ///
    /// If the version already exists, it returns the existing installation.
    ///
    /// **NOTE:** It doesn't verify the layout.
    pub fn add(&mut self, version: Version) -> &mut Install {
        let install = Install {
            layout: self.layout,
            path: self.path.join(version.to_string()),
            version,
        };

        self.installed.versions.entry(version).or_insert(install)
    }
}

/// Log paths.
#[derive(Clone, Debug)]
pub struct Logs {
    /// Path to the `logs` directory.
    pub path: PathBuf,
}

impl Logs {
    /// Constructs a log file `path` for a given [Bin].
    pub fn log_name(path: PathBuf, of: Bin) -> PathBuf {
        path.join(format!("{}.log", of.name_unix()))
    }
}

pub const SEGMENT_FILE_EXT: &str = ".stdb.log";

/// The `clog` directory contains the commit log files for the instance.
#[derive(Debug, Clone)]
pub struct CLog {
    /// Path to the `clog` directory.
    pub path: PathBuf,
}

impl CLog {
    /// By convention, the file name of a segment consists of the minimum
    /// transaction offset contained in it, left-padded with zeroes to 20 digits,
    /// and the file extension `.stdb.log`.
    pub fn segment_file_name(&self, offset: u64) -> PathBuf {
        self.path.join(format!("{offset:0>20}{SEGMENT_FILE_EXT}"))
    }
}

/// The `snapshots` directory contains the snapshots for the instance.
#[derive(Debug, Clone)]
pub struct Snapshots {
    /// Path to the `snapshots` directory.
    pub path: PathBuf,
}

/// `Path` to a specific spacetime instance.
#[derive(Clone)]
pub struct Replica {
    /// Path to the instance directory.
    pub path: PathBuf,
    /// The `instance id`.
    pub instance_id: u64,
    pub module_log: Logs,
    pub clog: CLog,
    pub snapshots: Snapshots,
}

impl Replica {
    /// Returns the `path` of the `scheduler` directory.
    ///
    /// NOTE: The scheduler directory is operated by `sled` so we don't assume any specific layout.
    pub fn scheduler_dir(&self) -> PathBuf {
        self.path.join("scheduler")
    }
}

impl fmt::Debug for Replica {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance")
            .field("path", &self.path)
            .field("instance_id", &self.instance_id)
            .field("clog", &self.clog)
            .field("module_logs_dir", &self.module_log)
            .field("scheduler_dir", &self.scheduler_dir())
            .field("snapshots", &self.snapshots)
            .finish()
    }
}

/// Collection of `Replica` paths.
#[derive(Clone)]
pub struct Replicas {
    /// Path to the `replicas` directory.
    pub path: PathBuf,
    // We want to keep the replicas sorted by `instance_id`.
    pub replicas: BTreeMap<u64, Replica>,
}

impl Replicas {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            replicas: Default::default(),
        }
    }

    /// Add a new replica with the given `instance_id`.
    ///
    /// If the replica already exists, it returns the existing replica.
    ///
    /// **NOTE:** It doesn't verify the layout.
    pub fn add(&mut self, instance_id: u64) -> &mut Replica {
        let path = self.path.join(instance_id.to_string());
        let replica = Replica {
            instance_id,
            module_log: Logs {
                path: path.join("logs"),
            },
            clog: CLog {
                path: path.join("clog"),
            },
            snapshots: Snapshots {
                path: path.join("snapshots"),
            },
            path,
        };

        self.replicas.entry(replica.instance_id).or_insert(replica)
    }
}

impl fmt::Debug for Replicas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Replicas")
            .field("path", &self.path)
            .field("replicas", &self.replicas.values())
            .finish()
    }
}
/// Data paths.
#[derive(Clone)]
pub struct Data {
    edition: EditionOptions,
    /// Path to the `data` directory.
    pub path: PathBuf,
    /// [Cache] paths.
    pub cache: Cache,
    /// [Logs] paths.
    pub logs: Option<Logs>,
    /// Directory containing the [Replicas].
    pub replicas: Replicas,
}

impl Data {
    /// Constructs a new `data` path with the given `path`.
    ///
    /// If `use_logs` is `true`, it creates the `logs` directory.
    pub fn new(path: PathBuf, edition: EditionOptions, use_logs: bool) -> Self {
        Self {
            edition,
            cache: Cache {
                path: path.join("cache"),
            },
            logs: if use_logs {
                Some(Logs {
                    path: path.join("logs"),
                })
            } else {
                None
            },
            replicas: Replicas::new(path.join("replicas")),
            path,
        }
    }
    /// Returns the path of the `config.toml` file.
    pub fn config_file(&self) -> PathBuf {
        self.path.join("config.toml")
    }
    /// Returns the path of the `metadata.toml` file.
    pub fn metadata_file(&self) -> PathBuf {
        self.path.join("metadata.toml")
    }
    /// Returns the path of the `PID` file.
    pub fn pid_file(&self) -> PathBuf {
        self.path.join("spacetime.pid")
    }
    /// Returns the path of the `program bytes` standalone directory.
    ///
    /// NOTE: Should be used for the _standalone edition_ only.
    pub fn program_bytes_standalone_dir(&self) -> Option<PathBuf> {
        if self.edition.kind() == EditionKind::StandAlone {
            Some(self.path.join("program-bytes"))
        } else {
            None
        }
    }
    /// Returns the path of the `control database` standalone directory.
    ///
    /// NOTE: Should be used for the _standalone edition_ only.
    pub fn control_db_standalone_dir(&self) -> Option<PathBuf> {
        if self.edition.kind() == EditionKind::StandAlone {
            Some(self.path.join("control-db"))
        } else {
            None
        }
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Data")
            .field("path", &self.path)
            .field("config_file", &self.config_file())
            .field("metadata_file", &self.metadata_file())
            .field("pid_file", &self.pid_file())
            .field("cache", &self.cache)
            .field("control_db_standalone_dir", &self.control_db_standalone_dir())
            .field("program_bytes_standalone_dir", &self.program_bytes_standalone_dir())
            .field("logs_dir", &self.logs)
            .field("replicas", &self.replicas)
            .finish()
    }
}

/// Generate the paths used by Spacetime.
///
/// WARNING: It only *calculates* the paths, not verify or create them.
#[derive(Clone)]
pub struct SpacetimePaths {
    pub edition: EditionOptions,
    pub root_dir: Option<PathBuf>,
    pub bin_file: PathBuf,
    pub bin: Binaries,
    pub config: Config,
    pub data: Data,
}

impl SpacetimePaths {
    /// Create a new instance of `SpacetimePaths` from the [Directories].
    pub fn new(edition: EditionOptions, dirs: Directories, use_logs: bool) -> Self {
        Self {
            root_dir: dirs.root_dir,
            bin_file: dirs.bin_file,
            bin: Binaries::new(dirs.bins_dir, dirs.layout),
            config: Config::new(dirs.config_dir),
            data: Data::new(dirs.data_dir, edition, use_logs),
            edition,
        }
    }
}

/// Options for creating a new [SpacetimeFs].
#[derive(Debug, Clone)]
pub struct FsOptions {
    edition: EditionOptions,
    /// Change the `root` directory.
    root: Option<PathBuf>,
    /// Change the `data` directory.
    data: Option<PathBuf>,
    /// If `true`, create the log directories.
    use_logs: bool,
}

impl FsOptions {
    pub fn standalone(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            edition: EditionOptions::StandAlone { major, minor, patch },
            root: None,
            data: None,
            use_logs: false,
        }
    }

    pub fn cloud(major: u64, minor: u64, patch: u64, address: Address) -> Self {
        Self {
            edition: EditionOptions::Cloud {
                major,
                minor,
                patch,
                address,
            },
            root: None,
            data: None,
            use_logs: false,
        }
    }

    /// Change the `root` directory.
    pub fn root(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
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

impl Fs for Install {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        self.cli_file().check_is_file()?;
        self.update_file().check_is_file()?;
        self.cli_template_file().check_is_file()?;
        self.config_template_file().check_is_file()?;

        let cloud_file = self.cloud_file();
        if cloud_file.exists() {
            cloud_file.check_is_file()?;
        }
        let standalone_file = self.standalone_file();
        if standalone_file.exists() {
            standalone_file.check_is_file()?;
        }
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        Ok(())
    }
}

impl Fs for Binaries {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        for install in self.installed.versions.values() {
            install.verify_layout()?;
        }
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        for install in self.installed.versions.values() {
            install.create_layout()?;
        }
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        for entry in glob_path(&self.path.clone(), "[0-9]*.[0-9]*.[0-9]*") {
            let entry = entry?;
            if entry.is_dir() {
                let install = self.add(Version::from_str(entry.file_name().unwrap().to_str().unwrap())?);
                install.load_layout()?;
            }
        }
        Ok(())
    }
}

impl Fs for Config {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        if self.cli_file().exists() {
            self.cli_file().check_is_file()?;
        }
        if self.public_key_file().exists() {
            self.public_key_file().check_is_file()?;
        }
        if self.private_key_file().exists() {
            self.private_key_file().check_is_file()?;
        }
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        Ok(())
    }
}

impl Fs for Cache {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        self.wasmtime_dir().check_is_dir()?;
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.wasmtime_dir())?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        Ok(())
    }
}

impl Fs for Logs {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        Ok(())
    }
}

impl Fs for Snapshots {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        Ok(())
    }
}

impl Fs for CLog {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        Ok(())
    }
}

impl Fs for Replica {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        self.clog.verify_layout()?;
        self.module_log.verify_layout()?;
        self.snapshots.verify_layout()?;

        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        self.clog.create_layout()?;
        self.module_log.create_layout()?;
        self.snapshots.create_layout()?;

        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        self.clog.load_layout()?;
        self.module_log.load_layout()?;
        self.snapshots.load_layout()?;

        Ok(())
    }
}

impl Fs for Replicas {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;
        for replica in self.replicas.values() {
            replica.verify_layout()?;
        }
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        for replica in self.replicas.values() {
            replica.create_layout()?;
        }
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        for entry in glob_path(&self.path.clone(), "[0-9]*") {
            let entry = entry?;
            if entry.is_dir() {
                let instance_id = entry.file_name().unwrap().to_str().unwrap().parse().unwrap();
                let replica = self.add(instance_id);
                replica.load_layout()?;
            }
        }

        Ok(())
    }
}

impl Fs for Data {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.path.check_is_dir()?;

        let metadata_file = self.metadata_file();

        let metadata = Metadata::read(&metadata_file)?;
        if metadata.edition.kind() != self.edition.kind() {
            return Err(ErrorPlatform::EditionMismatch {
                path: metadata_file,
                expected: self.edition.kind(),
                found: metadata.edition.kind(),
            });
        }
        if metadata.edition.version() < self.edition.version() {
            return Err(ErrorPlatform::VersionMismatch {
                path: metadata_file,
                expected: metadata.edition.version(),
                found: self.edition.version(),
            });
        }

        self.program_bytes_standalone_dir().check_is_dir()?;
        self.control_db_standalone_dir().check_is_dir()?;

        self.cache.verify_layout()?;
        self.replicas.verify_layout()?;
        if let Some(logs) = &self.logs {
            logs.verify_layout()?;
        }
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        create_dir(&self.path)?;
        self.cache.create_layout()?;
        self.replicas.create_layout()?;
        if let Some(logs) = &self.logs {
            logs.create_layout()?;
        }
        if let Some(program_bytes_standalone_dir) = self.program_bytes_standalone_dir() {
            create_dir(&program_bytes_standalone_dir)?;
        }
        if let Some(control_db_standalone_dir) = self.control_db_standalone_dir() {
            create_dir(&control_db_standalone_dir)?;
        }
        let metadata = Metadata { edition: self.edition };
        metadata.write(self.metadata_file())?;

        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        self.cache.load_layout()?;
        self.replicas.load_layout()?;
        if let Some(logs) = &mut self.logs {
            logs.load_layout()?;
        }
        Ok(())
    }
}

/// Spacetime file system.
///
/// It provides the paths used by Spacetime, and assert that the layout is correct.
#[derive(Debug, Clone)]
pub struct SpacetimeFs {
    paths: SpacetimePaths,
}

impl SpacetimeFs {
    /// Resolve the directories & files from the given [FsOptions].
    ///
    /// * Directories: `root`, `data`, `config`
    /// * Files: `config_client`, `config_server`
    ///
    /// Resolution order:
    ///
    /// The following is the order of precedence for a value defined in multiple places
    /// in increasing precedence:
    ///
    /// 1. The default value, if any is specified in the `spacetimedb-cli` code
    /// 2. The value in the `cli.toml` file
    /// 3. The value in the `.spacetime.toml` file (TODO)
    /// 4. The value specified in an environment variable (TODO)
    /// 5. The value specified as a CLI argument
    pub fn resolve(options: FsOptions) -> Result<Directories, ErrorPlatform> {
        let FsOptions {
            edition: _,
            root,
            data,
            use_logs: _,
        } = options;

        let mut dirs = Directories::platform();

        if let Some(root) = root {
            dirs = dirs.root(root);
        }
        if let Some(data) = data {
            dirs = dirs.data(data);
        }

        Ok(dirs)
    }

    /// Open an existing spacetime file system.
    ///
    /// It [Self::verify_layout] of the directories, but not create them.
    ///
    /// **NOTE:** It doesn't load the files that are part of the data, like logs, replicas, etc.
    pub fn open(options: FsOptions) -> Result<Self, ErrorPlatform> {
        let edition = options.edition;
        let use_logs = options.use_logs;
        let dirs = Self::resolve(options)?;

        let fs = Self {
            paths: SpacetimePaths::new(edition, dirs, use_logs),
        };
        fs.verify_layout()?;

        Ok(fs)
    }

    /// Create a new `SpacetimeFs` file system.
    ///
    /// It creates the directories and files that are part of the layout.
    ///
    /// **NOTE:** It doesn't create the files that are part of the data, like logs, replicas, etc.
    pub fn create(options: FsOptions) -> Result<Self, ErrorPlatform> {
        let edition = options.edition;
        let use_logs = options.use_logs;

        let dirs = Self::resolve(options)?;

        let fs = Self {
            paths: SpacetimePaths::new(edition, dirs, use_logs),
        };
        fs.create_layout()?;

        Ok(fs)
    }
}

impl Fs for SpacetimeFs {
    fn verify_layout(&self) -> Result<(), ErrorPlatform> {
        self.paths.root_dir.check_is_dir()?;
        if self.paths.bin_file.exists() {
            self.paths.bin_file.check_is_file()?;
        }
        self.paths.bin.verify_layout()?;
        self.paths.config.verify_layout()?;
        self.paths.data.verify_layout()?;
        Ok(())
    }

    fn create_layout(&self) -> Result<(), ErrorPlatform> {
        if let Some(root) = self.paths.root_dir.as_ref() {
            create_dir(root)?;
        }
        self.paths.bin.create_layout()?;
        self.paths.config.create_layout()?;
        self.paths.data.create_layout()?;
        Ok(())
    }

    fn load_layout(&mut self) -> Result<(), ErrorPlatform> {
        self.paths.bin.load_layout()?;
        self.paths.config.load_layout()?;
        self.paths.data.load_layout()?;

        Ok(())
    }
}

impl fmt::Debug for SpacetimePaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpacetimePaths")
            .field("edition", &self.edition)
            .field("root", &self.root_dir)
            .field("bin", &self.bin_file)
            .field("binaries", &self.bin)
            .field("config", &self.config)
            .field("data", &self.data)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use std::path::PathBuf;

    const LAYOUT_STANDALONE: &str = r#"SpacetimePaths {
    edition: StandAlone {
        major: 1,
        minor: 3,
        patch: 0,
    },
    root: Some(
        "/root",
    ),
    bin: "/root/spacetime",
    binaries: Binaries {
        layout: Custom(
            MacOs,
        ),
        path: "/root/bin",
        installed: Installed {
            versions: [
                Install {
                    path: "/root/bin/1.3.0",
                    version: Version {
                        major: 1,
                        minor: 3,
                        patch: 0,
                    },
                    cli_file: "/root/bin/1.3.0/spacetimedb-cli",
                    cloud_file: "/root/bin/1.3.0/spacetimedb-cloud",
                    update_file: "/root/bin/1.3.0/spacetimedb-update",
                    standalone_file: "/root/bin/1.3.0/spacetimedb-standalone",
                    cli_template_file: "/root/bin/1.3.0/cli.default.toml",
                    config_template_file: "/root/bin/1.3.0/config.default.toml",
                },
            ],
        },
    },
    config: Config {
        path: "/root/config",
        client_file: "/root/config/cli.toml",
        public_key_file: "/root/config/id_ecdsa.pub",
        private_key_file: "/root/config/id_ecdsa",
    },
    data: Data {
        path: "/root/data",
        config_file: "/root/data/config.toml",
        metadata_file: "/root/data/metadata.toml",
        pid_file: "/root/data/spacetime.pid",
        cache: Cache {
            path: "/root/data/cache",
            wasmtime_dir: "/root/data/cache/wasmtime",
        },
        control_db_standalone_dir: Some(
            "/root/data/control-db",
        ),
        program_bytes_standalone_dir: Some(
            "/root/data/program-bytes",
        ),
        logs_dir: Some(
            Logs {
                path: "/root/data/logs",
            },
        ),
        replicas: Replicas {
            path: "/root/data/replicas",
            replicas: [
                Instance {
                    path: "/root/data/replicas/1",
                    instance_id: 1,
                    clog: CLog {
                        path: "/root/data/replicas/1/clog",
                    },
                    module_logs_dir: Logs {
                        path: "/root/data/replicas/1/logs",
                    },
                    scheduler_dir: "/root/data/replicas/1/scheduler",
                    snapshots: Snapshots {
                        path: "/root/data/replicas/1/snapshots",
                    },
                },
            ],
        },
    },
}"#;

    const LAYOUT_CLOUD: &str = r#"SpacetimePaths {
    edition: Cloud {
        major: 1,
        minor: 3,
        patch: 0,
        address: Address(
            00000000000000000000000000000000,
        ),
    },
    root: Some(
        "/root",
    ),
    bin: "/root/spacetime",
    binaries: Binaries {
        layout: Custom(
            MacOs,
        ),
        path: "/root/bin",
        installed: Installed {
            versions: [
                Install {
                    path: "/root/bin/1.3.0",
                    version: Version {
                        major: 1,
                        minor: 3,
                        patch: 0,
                    },
                    cli_file: "/root/bin/1.3.0/spacetimedb-cli",
                    cloud_file: "/root/bin/1.3.0/spacetimedb-cloud",
                    update_file: "/root/bin/1.3.0/spacetimedb-update",
                    standalone_file: "/root/bin/1.3.0/spacetimedb-standalone",
                    cli_template_file: "/root/bin/1.3.0/cli.default.toml",
                    config_template_file: "/root/bin/1.3.0/config.default.toml",
                },
            ],
        },
    },
    config: Config {
        path: "/root/config",
        client_file: "/root/config/cli.toml",
        public_key_file: "/root/config/id_ecdsa.pub",
        private_key_file: "/root/config/id_ecdsa",
    },
    data: Data {
        path: "/root/data",
        config_file: "/root/data/config.toml",
        metadata_file: "/root/data/metadata.toml",
        pid_file: "/root/data/spacetime.pid",
        cache: Cache {
            path: "/root/data/cache",
            wasmtime_dir: "/root/data/cache/wasmtime",
        },
        control_db_standalone_dir: None,
        program_bytes_standalone_dir: None,
        logs_dir: Some(
            Logs {
                path: "/root/data/logs",
            },
        ),
        replicas: Replicas {
            path: "/root/data/replicas",
            replicas: [
                Instance {
                    path: "/root/data/replicas/1",
                    instance_id: 1,
                    clog: CLog {
                        path: "/root/data/replicas/1/clog",
                    },
                    module_logs_dir: Logs {
                        path: "/root/data/replicas/1/logs",
                    },
                    scheduler_dir: "/root/data/replicas/1/scheduler",
                    snapshots: Snapshots {
                        path: "/root/data/replicas/1/snapshots",
                    },
                },
            ],
        },
    },
}"#;

    #[test]
    fn correct_layout() {
        let dirs = Directories::custom_platform(PathBuf::from("/root"), Platform::MacOs);
        let mut fs = SpacetimePaths::new(
            EditionOptions::StandAlone {
                major: 1,
                minor: 3,
                patch: 0,
            },
            dirs.clone(),
            true,
        );
        fs.data.replicas.add(1);
        fs.bin.add(Version::new(1, 3, 0));
        assert_eq!(format!("{:#?}", fs), LAYOUT_STANDALONE);

        let mut fs = SpacetimePaths::new(
            EditionOptions::Cloud {
                major: 1,
                minor: 3,
                patch: 0,
                address: Default::default(),
            },
            dirs,
            true,
        );
        fs.data.replicas.add(1);
        fs.bin.add(Version::new(1, 3, 0));
        assert_eq!(format!("{:#?}", fs), LAYOUT_CLOUD);
    }

    fn check_base_paths(fs: &SpacetimeFs) {
        assert!(fs.paths.root_dir.as_deref().unwrap().exists());
        assert!(fs.paths.data.cache.path.exists());
        assert!(fs.paths.data.path.exists());
        assert!(fs.paths.data.metadata_file().exists());
        assert!(fs.paths.data.replicas.path.exists());
        if fs.paths.edition.kind() == EditionKind::StandAlone {
            assert!(fs.paths.data.control_db_standalone_dir().unwrap().exists());
            assert!(fs.paths.data.program_bytes_standalone_dir().unwrap().exists());
        }

        if let Some(logs) = &fs.paths.data.logs {
            assert!(logs.path.exists());
        }
    }

    fn check_data_paths(fs: &SpacetimeFs, instance_id: u64) {
        let replica = &fs.paths.data.replicas.replicas[&instance_id];
        assert!(replica.path.exists());
        assert!(replica.clog.path.exists());
        assert!(replica.module_log.path.exists());
        assert!(replica.snapshots.path.exists());
    }

    fn check_version_paths(fs: &SpacetimeFs, version: Version) {
        let bin = &fs.paths.bin.installed.versions[&version];
        assert!(bin.path.exists());
        //TODO:check files
    }

    #[test]
    fn fs_create() {
        let tmp = tempfile::tempdir().unwrap();
        let options = FsOptions::standalone(0, 1, 0).root(tmp.into_path());
        let version = Version::new(0, 1, 0);

        let mut fs = SpacetimeFs::create(options.clone()).unwrap();
        fs.paths.data.replicas.add(1);
        fs.paths.bin.add(version);
        fs.create_layout().unwrap();

        check_base_paths(&fs);
        check_data_paths(&fs, 1);
        check_version_paths(&fs, version);

        let fs = SpacetimeFs::create(options.use_logs(true)).unwrap();
        assert!(fs.paths.data.logs.unwrap().path.exists());
    }

    #[test]
    fn fs_open() {
        let tmp = tempfile::tempdir().unwrap();
        let version = Version::new(0, 1, 0);
        let options = FsOptions::standalone(0, 1, 0).root(tmp.into_path()).use_logs(true);

        let mut fs = SpacetimeFs::create(options.clone()).unwrap();
        fs.paths.data.replicas.add(1);
        fs.paths.bin.add(version);
        fs.create_layout().unwrap();

        let mut fs = SpacetimeFs::open(options).unwrap();
        fs.load_layout().unwrap();

        check_base_paths(&fs);
        check_data_paths(&fs, 1);
        check_version_paths(&fs, version);
    }
}
