use std::env::current_dir;
use std::fmt;
use std::path::PathBuf;

use crate::errors::ErrorPlatform;
use etcetera::base_strategy::{Windows, Xdg};
use etcetera::BaseStrategy;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
    Default,
    Cli,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    Xdg,
    Nix,
    Windows,
}

#[derive(Debug, Clone)]
pub struct PathArg {
    pub(crate) path: PathBuf,
    pub(crate) source: Source,
}

impl PathArg {
    pub(crate) fn exists(&self) -> bool {
        self.path.exists()
    }

    pub(crate) fn is_file(&self) -> Result<(), ErrorPlatform> {
        if !self.path.is_file() {
            return Err(ErrorPlatform::NotFile { path: self.clone() });
        }
        Ok(())
    }

    pub(crate) fn is_dir(&self) -> Result<(), ErrorPlatform> {
        if !self.path.is_dir() {
            return Err(ErrorPlatform::NotDirectory { path: self.clone() });
        }
        Ok(())
    }
}

impl From<PathBuf> for PathArg {
    fn from(path: PathBuf) -> Self {
        PathArg {
            path,
            source: Source::Default,
        }
    }
}

impl fmt::Display for PathArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {:?}", self.source, self.path)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PathArgOpt {
    pub(crate) path: Option<PathBuf>,
    pub(crate) source: Source,
}

impl PathArgOpt {
    pub(crate) fn none(source: Source) -> Self {
        PathArgOpt { path: None, source }
    }
    pub(crate) fn exists(&self) -> bool {
        self.path.as_ref().map_or(false, |path| path.exists())
    }
    pub(crate) fn is_file(&self) -> Result<(), ErrorPlatform> {
        if let Some(path) = &self.path {
            if !path.is_file() {
                return Err(ErrorPlatform::NotDirectory {
                    path: PathArg {
                        path: path.clone(),
                        source: self.source,
                    },
                });
            }
        }

        Ok(())
    }
    pub(crate) fn is_dir(&self) -> Result<(), ErrorPlatform> {
        if let Some(path) = &self.path {
            if !path.is_dir() {
                return Err(ErrorPlatform::NotDirectory {
                    path: PathArg {
                        path: path.clone(),
                        source: self.source,
                    },
                });
            }
        }

        Ok(())
    }
}

impl From<Option<PathBuf>> for PathArgOpt {
    fn from(path: Option<PathBuf>) -> Self {
        PathArgOpt {
            path,
            source: Source::Default,
        }
    }
}

#[derive(Debug)]
pub struct Directories {
    pub(crate) layout: Layout,
    pub(crate) root: PathArgOpt,
    pub(crate) bin: PathArg,
    pub(crate) config: PathArg,
    pub(crate) config_client: PathArgOpt,
    pub(crate) config_server: PathArgOpt,
    pub(crate) var: PathArg,
    pub(crate) data: PathArgOpt,
}

impl Directories {
    fn new(layout: Layout, root: PathBuf) -> Self {
        Self {
            layout,
            bin: root.join("bin").into(),
            config: root.join("config").into(),
            config_client: PathArgOpt::none(Source::Default),
            config_server: PathArgOpt::none(Source::Default),
            var: root.join("var").into(),
            root: Some(root).into(),
            data: None.into(),
        }
    }

    /// Create a new `Directories` with the given `root` path.
    pub fn custom(root: PathBuf) -> Self {
        let layout = if cfg!(target_os = "windows") {
            Layout::Windows
        } else {
            Layout::Nix
        };
        Self::new(layout, root)
    }

    /// Create a new `Directories` with the default paths for `macOS/Linux` systems.
    pub fn nix() -> Self {
        let dir = Xdg::new().unwrap();
        let home = dir.home_dir();
        Self::new(Layout::Nix, home.join(".spacetime"))
    }

    /// Create a new `Directories` with the default paths for `Windows` systems.
    pub fn windows() -> Self {
        let dir = Windows::new().unwrap();
        let home = dir.home_dir();
        Self::new(Layout::Windows, home.join("SpacetimeDB"))
    }

    /// Create a new `Directories` with the default paths for `Xdg` systems.
    pub fn xdg() -> Self {
        let dir = Xdg::new().unwrap();

        Self {
            layout: Layout::Xdg,
            root: None.into(),
            bin: dir.home_dir().join(".local").join("bin").into(),
            config: dir.config_dir().join("spacetime").into(),
            config_client: PathArgOpt::none(Source::Default),
            config_server: PathArgOpt::none(Source::Default),
            var: dir.data_dir().join("spacetime").into(),
            data: None.into(),
        }
    }

    /// Create a new `Directories` from the current working directory, using a `Nix` layout.
    pub fn current_dir() -> Result<Self, ErrorPlatform> {
        let root = current_dir().map_err(|error| ErrorPlatform::IO {
            path: Default::default(),
            error,
        })?;
        Ok(Self::new(Layout::Nix, root.join(".spacetime")))
    }

    /// Create a new `Directories` based on the current platform.
    pub fn platform() -> Self {
        if cfg!(target_os = "windows") {
            Directories::windows()
        } else {
            Directories::xdg()
        }
    }

    pub(crate) fn data_dir(&self) -> PathBuf {
        self.data.path.clone().unwrap_or_else(|| self.var.path.join("data"))
    }

    /// Change the `root` path.
    ///
    /// NOTE: Recalculate the other `paths` based on the new root
    pub fn root(mut self, source: Source, path: PathBuf) -> Self {
        // Save the old settings
        let data = self.data.clone();
        let config = self.config.clone();

        self = Self::new(self.layout, path);
        self.root.source = source;
        self.data = data;
        self.config = config;
        self
    }

    /// Change the `bin` path.
    pub fn config_dir(mut self, source: Source, path: PathBuf) -> Self {
        self.config = PathArg { path, source };
        self
    }

    /// Change to a custom client `config` file.
    pub fn config_client_file(mut self, source: Source, path: PathBuf) -> Self {
        self.config_client = PathArgOpt {
            path: Some(path),
            source,
        };
        self
    }

    /// Change to a custom server `config` file.
    pub fn config_server_file(mut self, source: Source, path: PathBuf) -> Self {
        self.config_server = PathArgOpt {
            path: Some(path),
            source,
        };
        self
    }

    /// Change the `data` path.
    pub fn data(mut self, source: Source, path: PathBuf) -> Self {
        self.data = PathArgOpt {
            path: Some(path),
            source,
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg() {
        let dirs = Directories::xdg();

        assert!(dirs.root.path.is_none());
        assert!(dirs.bin.path.ends_with(".local/bin"));
        assert!(dirs.config.path.ends_with("spacetime"));
        assert!(dirs.config_client.path.is_none());
        assert!(dirs.config_server.path.is_none());
        assert!(dirs.var.path.ends_with("spacetime"));
    }

    #[test]
    fn nix() {
        let dirs = Directories::nix();

        assert!(dirs.root.path.unwrap().ends_with(".spacetime"));
        assert!(dirs.bin.path.ends_with(".spacetime/bin"));
        assert!(dirs.config.path.ends_with(".spacetime/config"));
        assert!(dirs.config_client.path.is_none());
        assert!(dirs.config_server.path.is_none());
        assert!(dirs.var.path.ends_with(".spacetime/var"));
    }

    #[test]
    fn windows() {
        let dirs = Directories::windows();

        assert!(dirs.root.path.unwrap().ends_with("SpacetimeDB"));
        assert!(dirs.bin.path.ends_with("SpacetimeDB/bin"));
        assert!(dirs.config.path.ends_with("SpacetimeDB/config"));
        assert!(dirs.config_client.path.is_none());
        assert!(dirs.config_server.path.is_none());
        assert!(dirs.var.path.ends_with("SpacetimeDB/var"));
    }

    #[test]
    fn custom() {
        let custom_path = PathBuf::from("/custom/path");
        let dirs = Directories::custom(custom_path.clone());

        assert_eq!(dirs.root.path.clone().unwrap(), custom_path);
        assert!(dirs.bin.path.ends_with("bin"));
        assert!(dirs.config.path.ends_with("config"));
        assert!(dirs.config_client.path.is_none());
        assert!(dirs.config_server.path.is_none());
        assert!(dirs.var.path.ends_with("var"));

        // Changing the config file
        let dirs = dirs
            .config_dir(Source::Cli, PathBuf::from("/new/config"))
            .config_client_file(Source::Cli, PathBuf::from("/a.toml"))
            .config_server_file(Source::Cli, PathBuf::from("/b.toml"));
        assert_eq!(dirs.config.path, PathBuf::from("/new/config"));
        assert!(dirs.config_client.path.unwrap().ends_with("a.toml"));
        assert!(dirs.config_server.path.unwrap().ends_with("b.toml"));
    }

    // Testing that the paths are correctly set when by the `builder` methods,
    // and that changing the `root` also changes the others.
    #[test]
    fn setting_paths() {
        let initial_path = PathBuf::from("/initial/path");
        let new_root = PathBuf::from("/new/root");
        let dirs = Directories::custom(initial_path.clone());

        let dirs = dirs.config_dir(Source::Cli, PathBuf::from("/new/config"));

        assert_eq!(dirs.config.path, PathBuf::from("/new/config"));
        assert_eq!(dirs.config.source, Source::Cli);
        assert_eq!(dirs.root.path.as_deref().unwrap(), &initial_path);

        let dirs = dirs.data(Source::Config, PathBuf::from("/new/data"));

        assert_eq!(dirs.data.source, Source::Config);
        assert_eq!(dirs.data.path.as_deref().unwrap(), &PathBuf::from("/new/data"));

        let dirs = dirs.root(Source::Config, new_root.clone());

        assert_eq!(dirs.root.source, Source::Config);
        assert_eq!(dirs.root.path.unwrap(), new_root);
        assert_eq!(dirs.bin.path, PathBuf::from("/new/root/bin"));

        assert_eq!(dirs.config.source, Source::Cli);
        assert_eq!(dirs.config.path, PathBuf::from("/new/config"));

        assert_eq!(dirs.data.source, Source::Config);
        assert_eq!(dirs.data.path.unwrap(), PathBuf::from("/new/data"));
    }
}
