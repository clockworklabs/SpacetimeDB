//! # Directory Structure of the Database.
//!
//! The [Directories] holds the paths to the various directories used by the database.
//!
//! *  **cli-bin-dir** We defined a `cli-bin-dir` under which all versions of all
//!  SpacetimeDB binaries will be stored. Each binary will be stored in a
//!  directory named with version number of the binary in this directory. If a
//!  binary has any related files required by that binary which are specific to
//!  that version, for example, template configuration files, these files will be
//!  installed in this folder as well.
//!
//! *  **cli-config-dir** We define a `cli-config-dir` which is where
//! configuration and state for the CLI will be stored.
//!
//! ## Linux/macOS Directory Structure
//!
//! On Linux and macOS the installation paths follow the `XDG` conventions by default:
//!
//! * Default `cli-config-dir`: `$HOME/.config/spacetime`
//! * Default `cli-bin-dir`: `$HOME/.local/share/spacetime/bin`
//! * Default `cli-bin-file`: `$HOME/.local/bin/spacetime`
//!
//! We also observe the `XDG` environment variables if they are set. If they are set
//! then the paths are defined as:
//!
//! * Default `cli-config-dir`: `$XDG_CONFIG_HOME/spacetime`
//! * Default `cli-bin-dir`: `$XDG_DATA_HOME/spacetime/bin`
//!
//! For reference, the below is an example installation using the default paths:
//!
//!```bash
//! $HOME
//! ├── .local
//! │   ├── bin
//! │   │   └── spacetime -> $HOME/.local/share/spacetime/bin/1.10.1/spacetimedb-update # Current, in $PATH
//! │   └── share
//! │       └── spacetime
//! │           └── bin
//! │               └── 1.10.1
//! │                   ├── spacetimedb-update # Version manager
//! │                   ├── spacetimedb-cli # CLI
//! │                   ├── spacetimedb-standalone # Server
//! │                   ├── spacetimedb-cloud # Server
//! │                   ├── cli.default.toml # Template CLI configuration file
//! │                   └── config.default.toml # Template server configuration file
//! └── .config
//!     └── spacetime
//!         ├── id_ecdsa # Private key
//!         ├── id_ecdsa.pub # Public key
//!         └── cli.toml # CLI configuration
//! ```
//!
//!## Windows Directory Structure
//!
//! On Windows the installation paths follow Windows conventions:
//!
//! * Default `cli-config-dir`: `%LocalAppData%\SpacetimeDB\config`
//! * Default `cli-bin-dir`: `%LocalAppData%\SpacetimeDB\bin`
//! * Default `cli-bin-file`: `%LocalAppData%\SpacetimeDB\spacetime.exe`
//!
//! > **Note**: Both directories use `%LocalAppData%` and not `%AppData%`. This is
//! > intentional so that different users on Windows can have different configuration
//! > and binaries. This also allows you to install SpacetimeDB on Windows even if you
//! > are not a privileged user.
//!
//! For reference, the below is an example installation using the default paths:
//!
//! ```bash
//! %LocalAppData%
//! └── SpacetimeDB
//!     ├── spacetime.exe # A copy of .\bin\1.10.1\spacetimedb-update.exe
//!     ├── config
//!     │   ├── id_ecdsa # Private key
//!     │   ├── id_ecdsa.pub # Public key
//!     │   └── cli.toml # CLI configuration
//!     └── bin
//!         └── 1.10.1
//!             ├── spacetimedb-update.exe # Version manager
//!             ├── spacetimedb-cli.exe # CLI
//!             ├── spacetimedb-standalone.exe # Server
//!             ├── spacetimedb-cloud.exe # Server
//!             ├── cli.default.toml # Template CLI configuration file
//!             └── config.default.toml # Template server configuration file
//! ```
//!
//! ## Custom Root Directory
//!
//! Users on all platforms must be allowed to override the default installation
//! paths entirely with a single `--root-dir` argument passed to the initial
//! installation commands.
//!
//! If users specify a `--root-dir` flag, then the installation paths should be
//! defined relative to the `root-dir` as follows:
//!
//! * `cli-config-dir`: `{root-dir}/config`
//! * `cli-bin-dir`: `{root-dir}/bin`
//! * `cli-bin-file`: `{root-dir}/spacetime`
//!
//! For reference, the below is an example installation using the `--root-dir` argument:
//!
//! ```bash
//! {root-dir}
//! ├── spacetime -> {root-dir}/bin/1.10.1/spacetimedb-update # Current, in $PATH
//! ├── config
//! │   ├── id_ecdsa # Private key
//! │   ├── id_ecdsa.pub # Public key
//! │   └── cli.toml # CLI configuration
//! └── bin
//!     └── 1.10.1
//!         ├── spacetimedb-update.exe # Version manager
//!         ├── spacetimedb-cli.exe # CLI
//!         ├── spacetimedb-standalone.exe # Server
//!         ├── spacetimedb-cloud.exe # Server
//!         ├── cli.default.toml # Template CLI configuration file
//!         └── config.default.toml # Template server configuration file
//! ```

use std::path::PathBuf;

use etcetera::base_strategy::{Windows, Xdg};
use etcetera::BaseStrategy;

use crate::errors::ErrorPlatform;
use crate::metadata::Bin;
use crate::platform::Platform;

/// The `Layout` enum represents the different directories layouts that the database can be
/// installed in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    /// The `Xdg` layout is the default layout for Unix-like systems.
    Xdg,
    /// The `Custom` layout is for custom installations.
    Custom(Platform),
    /// The `Windows` layout is the default layout for Windows systems.
    Windows,
}

impl Layout {
    /// Get the current layout based on the `target_os`.
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Layout::Windows
        } else {
            Layout::Xdg
        }
    }

    /// Get the current [Platform] based on the layout.
    pub fn platform(&self) -> Platform {
        match self {
            Layout::Xdg => Platform::current(),
            Layout::Custom(platform) => *platform,
            Layout::Windows => Platform::Windows,
        }
    }
}

/// The `CheckPath` trait provides methods to check if a path is a file or a directory,
/// and turns the errors into `ErrorPlatform`.
pub trait CheckPath {
    /// Check if the `path` is a file.
    fn check_is_file(&self) -> Result<(), ErrorPlatform>;
    /// Check if the `path` is a directory.
    fn check_is_dir(&self) -> Result<(), ErrorPlatform>;
}

impl CheckPath for PathBuf {
    fn check_is_file(&self) -> Result<(), ErrorPlatform> {
        if !self.is_file() {
            return Err(ErrorPlatform::NotFile { path: self.clone() });
        }
        Ok(())
    }

    fn check_is_dir(&self) -> Result<(), ErrorPlatform> {
        if !self.is_dir() {
            return Err(ErrorPlatform::NotDirectory { path: self.clone() });
        }
        Ok(())
    }
}

impl CheckPath for Option<PathBuf> {
    fn check_is_file(&self) -> Result<(), ErrorPlatform> {
        self.as_ref().map_or(Ok(()), |path| path.check_is_file())
    }

    fn check_is_dir(&self) -> Result<(), ErrorPlatform> {
        self.as_ref().map_or(Ok(()), |path| path.check_is_dir())
    }
}

/// The [Directories] struct holds the `paths` to the various directories used by the database.
///
/// **NOTE**: This not create the directories, only calculates the `paths`.
#[derive(Debug, Clone)]
pub struct Directories {
    pub layout: Layout,
    pub home_dir: PathBuf,
    pub root_dir: Option<PathBuf>,
    /// The `cli-bin-file` file.
    pub bin_file: PathBuf,
    /// The `cli-bin-dir` directory.
    pub bins_dir: PathBuf,
    /// The `cli-config-dir` directory.
    pub config_dir: PathBuf,
    /// The `cli-data-dir` directory.
    pub data_dir: PathBuf,
}

impl Directories {
    fn new(layout: Layout, root: PathBuf) -> Self {
        Self {
            layout,
            bin_file: root.join(Bin::Spacetime.name(layout)),
            bins_dir: root.join("bin"),
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            root_dir: Some(root),
            home_dir: etcetera::home_dir().unwrap(),
        }
    }

    /// Create a new [Directories] with the given `root` path and [Platform].
    pub fn custom_platform(root: PathBuf, platform: Platform) -> Self {
        Self::new(Layout::Custom(platform), root)
    }

    /// Create a new [Directories] with the given `root` path, and the *current* [Platform].
    pub fn custom(root: PathBuf) -> Self {
        Self::custom_platform(root, Platform::current())
    }

    /// Create a new [Directories] with the default paths for `Windows` systems.
    pub fn windows() -> Self {
        let dir = Windows::new().unwrap();
        let home = dir.home_dir();
        Self::new(Layout::Windows, home.join("SpacetimeDB"))
    }

    /// Create a new [Directories] with the default paths for `Xdg` systems.
    pub fn xdg() -> Self {
        let dir = Xdg::new().unwrap();
        let local = dir.home_dir().join(".local");
        let share = local.join("share").join("spacetime");
        Self {
            layout: Layout::Xdg,
            home_dir: dir.home_dir().to_path_buf(),
            root_dir: None,
            bin_file: local.join("bin").join(Bin::Spacetime.name(Layout::Xdg)),
            bins_dir: share.join("bin"),
            config_dir: dir.config_dir().join("spacetime"),
            data_dir: share.join("data"),
        }
    }

    /// Create a new [Directories] based on the current platform.
    pub fn platform() -> Self {
        if cfg!(target_os = "windows") {
            Directories::windows()
        } else {
            Directories::xdg()
        }
    }

    /// Change the `root` path.
    ///
    /// NOTE: Recalculate the other `paths` based on the new `root_dir`.
    pub fn root(self, root_dir: PathBuf) -> Self {
        Self::new(Layout::Custom(Platform::current()), root_dir)
    }

    /// Change the `data` path.
    pub fn data(mut self, data_dir: PathBuf) -> Self {
        self.data_dir = data_dir;
        self
    }

    /// Utility method to remove the `home` directory from the paths.
    fn without_home(mut self) -> Self {
        let remove_home = |path: PathBuf| -> PathBuf { path.strip_prefix(&self.home_dir).unwrap().to_path_buf() };

        self.root_dir = self.root_dir.map(remove_home);
        self.bin_file = remove_home(self.bin_file);
        self.bins_dir = remove_home(self.bins_dir);
        self.config_dir = remove_home(self.config_dir);
        self.data_dir = remove_home(self.data_dir);

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg() {
        let dirs = Directories::xdg().without_home();
        assert_eq!(dirs.root_dir, None);
        assert_eq!(dirs.bin_file, PathBuf::from(".local").join("bin").join("spacetime"));
        assert_eq!(
            dirs.bins_dir,
            PathBuf::from(".local").join("share").join("spacetime").join("bin")
        );
        assert_eq!(dirs.config_dir, PathBuf::from(".config").join("spacetime"));
        assert_eq!(
            dirs.data_dir,
            PathBuf::from(".local").join("share").join("spacetime").join("data")
        );
    }

    #[test]
    fn windows() {
        let dirs = Directories::windows().without_home();
        assert_eq!(dirs.root_dir, Some(PathBuf::from("SpacetimeDB")));
        assert_eq!(dirs.bin_file, PathBuf::from("SpacetimeDB").join("spacetime.exe"));
        assert_eq!(dirs.bins_dir, PathBuf::from("SpacetimeDB").join("bin"));
        assert_eq!(dirs.config_dir, PathBuf::from("SpacetimeDB").join("config"));
        assert_eq!(dirs.data_dir, PathBuf::from("SpacetimeDB").join("data"));
    }

    #[test]
    fn custom() {
        let custom_path = PathBuf::from("custom").join("path");
        let dirs = Directories::custom(custom_path.clone());

        assert_eq!(dirs.root_dir, Some(custom_path.clone()));
        assert_eq!(dirs.bin_file, custom_path.join("spacetime"));
        assert_eq!(dirs.bins_dir, custom_path.join("bin"));
        assert_eq!(dirs.config_dir, custom_path.join("config"));
        assert_eq!(dirs.data_dir, custom_path.join("data"));
    }

    // Testing that the paths are correctly set by the `builder` methods,
    // and that changing the `root` also changes the others.
    #[test]
    fn setting_paths() {
        let dirs = Directories::custom(PathBuf::from("initial").join("path"));

        // Change the `data` path
        let dirs = dirs.data(PathBuf::from("new").join("data"));
        assert_eq!(dirs.data_dir, PathBuf::from("new").join("data"));

        // Change the root
        let dirs = dirs.root(PathBuf::from("new").join("root"));

        assert_eq!(dirs.root_dir, Some(PathBuf::from("new").join("root")));

        assert_eq!(dirs.bin_file, PathBuf::from("new").join("root").join("spacetime"));
        assert_eq!(dirs.bins_dir, PathBuf::from("new").join("root").join("bin"));
        assert_eq!(dirs.config_dir, PathBuf::from("new").join("root").join("config"));
        assert_eq!(dirs.data_dir, PathBuf::from("new").join("root").join("data"));
    }
}
