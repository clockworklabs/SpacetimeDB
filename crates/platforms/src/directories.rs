use std::fmt;
use std::path::PathBuf;

use crate::errors::ErrorPlatform;
use crate::files::{CONFIG_CLIENT, CONFIG_SERVER};
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
pub struct BaseDirectories {
    pub(crate) layout: Layout,
    pub(crate) root: PathArgOpt,
    pub(crate) bin: PathArg,
    pub(crate) config: PathArg,
    pub(crate) config_client: PathArg,
    pub(crate) config_server: PathArg,
    pub(crate) var: PathArg,
    pub(crate) data: PathArgOpt,
}

impl BaseDirectories {
    fn new(layout: Layout, dir: PathBuf) -> BaseDirectories {
        BaseDirectories {
            layout,
            bin: dir.join("bin").into(),
            config: dir.join("config").into(),
            config_client: dir.join("config").join(CONFIG_CLIENT).into(),
            config_server: dir.join("config").join(CONFIG_SERVER).into(),
            var: dir.join("var").into(),
            root: Some(dir).into(),
            data: None.into(),
        }
    }

    pub fn custom(dir: PathBuf) -> BaseDirectories {
        let layout = if cfg!(target_os = "windows") {
            Layout::Windows
        } else {
            Layout::Nix
        };
        Self::new(layout, dir)
    }

    pub fn nix() -> BaseDirectories {
        let dir = Xdg::new().unwrap();
        let home = dir.home_dir();
        Self::new(Layout::Nix, home.join(".spacetime"))
    }

    pub fn windows() -> BaseDirectories {
        let dir = Windows::new().unwrap();
        let home = dir.home_dir();
        Self::new(Layout::Windows, home.join("SpacetimeDB"))
    }

    pub fn xdg() -> BaseDirectories {
        let dir = Xdg::new().unwrap();

        BaseDirectories {
            layout: Layout::Xdg,
            root: None.into(),
            bin: dir.home_dir().join(".local").join("bin").into(),
            config: dir.config_dir().join("spacetime").into(),
            config_client: dir.config_dir().join("spacetime").join(CONFIG_CLIENT).into(),
            config_server: dir.config_dir().join("spacetime").join(CONFIG_SERVER).into(),
            var: dir.data_dir().join("spacetime").into(),
            data: None.into(),
        }
    }

    pub fn platform() -> BaseDirectories {
        if cfg!(target_os = "windows") {
            BaseDirectories::windows()
        } else {
            BaseDirectories::xdg()
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data.path.clone().unwrap_or_else(|| self.var.path.join("data"))
    }

    /// Recalculate the paths based on the new root
    pub fn with_root(self, source: Source, path: PathBuf) -> Self {
        // Save the old settings
        let data = self.data.clone();
        let config = self.config.clone();

        let mut x = Self::new(self.layout, path);
        x.root.source = source;
        x.data = data;
        x.config = config;
        x
    }

    pub fn with_config_dir(self, source: Source, path: PathBuf) -> Self {
        let mut x = self;
        x.config = PathArg { path, source };
        x
    }

    pub fn with_config_client_file(self, source: Source, path: PathBuf) -> Self {
        let mut x = self;
        x.config_client = PathArg { path, source };
        x
    }

    pub fn with_config_server_file(self, source: Source, path: PathBuf) -> Self {
        let mut x = self;
        x.config_server = PathArg { path, source };
        x
    }

    pub fn with_data(self, source: Source, path: PathBuf) -> Self {
        let mut x = self;
        x.data = PathArgOpt {
            path: Some(path),
            source,
        };
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg() {
        let dirs = BaseDirectories::xdg();

        assert!(dirs.root.path.is_none());
        assert!(dirs.bin.path.ends_with(".local/bin"));
        assert!(dirs.config.path.ends_with("spacetime"));
        assert!(dirs.config_client.path.ends_with("spacetime/client.toml"));
        assert!(dirs.config_server.path.ends_with("spacetime/server.toml"));
        assert!(dirs.var.path.ends_with("spacetime"));
    }

    #[test]
    fn nix() {
        let dirs = BaseDirectories::nix();

        assert!(dirs.root.path.unwrap().ends_with(".spacetime"));
        assert!(dirs.bin.path.ends_with(".spacetime/bin"));
        assert!(dirs.config.path.ends_with(".spacetime/config"));
        assert!(dirs.config_client.path.ends_with("client.toml"));
        assert!(dirs.config_server.path.ends_with("server.toml"));
        assert!(dirs.var.path.ends_with(".spacetime/var"));
    }

    #[test]
    fn windows() {
        let dirs = BaseDirectories::windows();

        assert!(dirs.root.path.unwrap().ends_with("SpacetimeDB"));
        assert!(dirs.bin.path.ends_with("SpacetimeDB/bin"));
        assert!(dirs.config.path.ends_with("SpacetimeDB/config"));
        assert!(dirs.config_client.path.ends_with("client.toml"));
        assert!(dirs.config_server.path.ends_with("server.toml"));
        assert!(dirs.var.path.ends_with("SpacetimeDB/var"));
    }

    #[test]
    fn custom() {
        let custom_path = PathBuf::from("/custom/path");
        let dirs = BaseDirectories::custom(custom_path.clone());

        assert_eq!(dirs.root.path.clone().unwrap(), custom_path);
        assert!(dirs.bin.path.ends_with("bin"));
        assert!(dirs.config.path.ends_with("config"));
        assert!(dirs.config_client.path.ends_with("client.toml"));
        assert!(dirs.config_server.path.ends_with("server.toml"));
        assert!(dirs.var.path.ends_with("var"));

        // Changing the config file
        let dirs = dirs
            .with_config_dir(Source::Cli, PathBuf::from("/new/config"))
            .with_config_client_file(Source::Cli, PathBuf::from("/a.toml"))
            .with_config_server_file(Source::Cli, PathBuf::from("/b.toml"));
        assert_eq!(dirs.config.path, PathBuf::from("/new/config"));
        assert!(dirs.config_client.path.ends_with("a.toml"));
        assert!(dirs.config_server.path.ends_with("b.toml"));
    }

    // Testing that the paths are correctly set when changing any of the pats that could
    // be changed by the `with_*` methods, and that changing the root also changes the
    // other paths.
    #[test]
    fn setting_paths() {
        let initial_path = PathBuf::from("/initial/path");
        let new_root = PathBuf::from("/new/root");
        let dirs = BaseDirectories::custom(initial_path.clone());

        let dirs = dirs.with_config_dir(Source::Cli, PathBuf::from("/new/config"));

        assert_eq!(dirs.config.path, PathBuf::from("/new/config"));
        assert_eq!(dirs.config.source, Source::Cli);
        assert_eq!(dirs.root.path.as_deref().unwrap(), &initial_path);

        let dirs = dirs.with_data(Source::Config, PathBuf::from("/new/data"));

        assert_eq!(dirs.data.source, Source::Config);
        assert_eq!(dirs.data.path.as_deref().unwrap(), &PathBuf::from("/new/data"));

        let dirs = dirs.with_root(Source::Config, new_root.clone());

        assert_eq!(dirs.root.source, Source::Config);
        assert_eq!(dirs.root.path.unwrap(), new_root);
        assert_eq!(dirs.bin.path, PathBuf::from("/new/root/bin"));

        assert_eq!(dirs.config.source, Source::Cli);
        assert_eq!(dirs.config.path, PathBuf::from("/new/config"));

        assert_eq!(dirs.data.source, Source::Config);
        assert_eq!(dirs.data.path.unwrap(), PathBuf::from("/new/data"));
    }
}
