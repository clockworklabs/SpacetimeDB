use crate::util::is_hex_identity;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdentityConfig {
    pub nickname: Option<String>,
    pub identity: String,
    pub token: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RawConfig {
    host: Option<String>,
    protocol: Option<String>,
    default_identity: Option<String>,
    default_address: Option<String>,
    identity_configs: Option<Vec<IdentityConfig>>,
}

#[derive(Clone)]
pub struct Config {
    proj: RawConfig,
    home: RawConfig,
}

const HOME_CONFIG_DIR: &str = ".spacetime";
const CONFIG_FILENAME: &str = "config.toml";
const SPACETIME_FILENAME: &str = "spacetime.toml";
const DOT_SPACETIME_FILENAME: &str = ".spacetime.toml";

const DEFAULT_HOST: &str = "spacetimedb.com/spacetimedb";
const DEFAULT_PROTOCOL: &str = "https";

impl Config {
    pub fn host(&self) -> String {
        if let Ok(host) = std::env::var("SPACETIMEDB_HOST") {
            host
        } else {
            self.proj
                .host
                .as_ref()
                .or(self.home.host.as_ref())
                .map(|s| s.as_str())
                .unwrap_or(DEFAULT_HOST)
                .to_owned()
        }
    }

    pub fn set_host(&mut self, host: &str) {
        self.home.host = Some(host.to_string());
    }

    pub fn protocol(&self) -> String {
        if let Ok(protocol) = std::env::var("SPACETIMEDB_PROTOCOL") {
            protocol
        } else {
            self.proj
                .protocol
                .as_ref()
                .or(self.home.protocol.as_ref())
                .map(|s| s.as_str())
                .unwrap_or(DEFAULT_PROTOCOL)
                .to_owned()
        }
    }

    pub fn set_protocol(&mut self, protocol: &str) {
        self.home.protocol = Some(protocol.to_string());
    }

    pub fn default_identity(&self) -> Option<&str> {
        self.proj
            .default_identity
            .as_ref()
            .or(self.home.default_identity.as_ref())
            .map(|s| s.as_str())
    }

    pub fn set_default_identity(&mut self, default_identity: String) {
        self.home.default_identity = Some(default_identity);
    }

    /// Sets the `nickname` for the provided `identity`.
    ///
    /// If the `identity` already has a `nickname` set, it will be overwritten and returned. If the
    /// `identity` is not found, an error will be returned.
    ///
    /// # Returns
    /// * `Ok(Option<String>)` - If the identity was found, the old nickname will be returned.
    /// * `Err(anyhow::Error)` - If the identity was not found.
    pub fn set_identity_nickname(&mut self, identity: &str, nickname: &str) -> Result<Option<String>, anyhow::Error> {
        match &mut self.home.identity_configs {
            None => {
                panic!("Identity {} not found", identity);
            }
            Some(ref mut configs) => {
                let config = configs
                    .iter_mut()
                    .find(|c| c.identity == identity)
                    .ok_or_else(|| anyhow::anyhow!("Identity {} not found", identity))?;
                let old_nickname = config.nickname.clone();
                config.nickname = Some(nickname.to_string());
                Ok(old_nickname)
            }
        }
    }

    pub fn default_address(&self) -> Option<&str> {
        self.proj
            .default_identity
            .as_ref()
            .or(self.home.default_address.as_ref())
            .map(|s| s.as_str())
    }

    pub fn identity_configs(&self) -> &Vec<IdentityConfig> {
        self.home.identity_configs.as_ref().unwrap()
    }

    pub fn identity_configs_mut(&mut self) -> &mut Vec<IdentityConfig> {
        self.home.identity_configs.get_or_insert(vec![])
    }

    fn find_config_filename(config_dir: &PathBuf) -> Option<&'static str> {
        let read_dir = fs::read_dir(config_dir).unwrap();
        let filenames = [DOT_SPACETIME_FILENAME, SPACETIME_FILENAME, CONFIG_FILENAME];
        let mut config_filename = None;
        'outer: for path in read_dir {
            for name in filenames {
                if name == path.as_ref().unwrap().file_name().to_str().unwrap() {
                    config_filename = Some(name);
                    break 'outer;
                }
            }
        }
        config_filename
    }

    fn load_raw(config_dir: PathBuf) -> RawConfig {
        if let Some(config_path) = std::env::var_os("SPACETIME_CONFIG_FILE") {
            return Self::load_from_file(config_path.as_ref());
        }
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).unwrap();
        }

        let config_filename = Self::find_config_filename(&config_dir);
        let Some(config_filename) = config_filename else {
            // Return an empty raw config without creating a file.
            return toml::from_str("").unwrap();
        };

        let config_path = config_dir.join(config_filename);
        Self::load_from_file(&config_path)
    }

    fn load_from_file(config_path: &Path) -> RawConfig {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(config_path)
            .unwrap();

        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        toml::from_str(&text).unwrap()
    }

    pub fn load() -> Self {
        let home_dir = dirs::home_dir().unwrap();
        let mut home_config = Self::load_raw(home_dir.join(HOME_CONFIG_DIR));

        // Ensure there is always an identity config. Simplifies other code.
        home_config.identity_configs.get_or_insert(vec![]);

        // TODO(cloutiertyler): For now we're checking for a spacetime.toml file
        // in the current directory. Eventually this should really be that we
        // search parent directories above the current directory to find
        // spacetime.toml files like a .gitignore file
        let cur_dir = std::env::current_dir().expect("No current working directory!");
        let cur_config = Self::load_raw(cur_dir);

        Self {
            home: home_config,
            proj: cur_config,
        }
    }

    pub fn save(&self) {
        let home_dir = dirs::home_dir().unwrap();
        let config_dir = home_dir.join(HOME_CONFIG_DIR);
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).unwrap();
        }

        let config_filename = Self::find_config_filename(&config_dir).unwrap_or(CONFIG_FILENAME);

        let config_path = config_dir.join(config_filename);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(config_path)
            .unwrap();

        let str = toml::to_string_pretty(&self.home).unwrap();

        file.set_len(0).unwrap();
        file.write_all(str.as_bytes()).unwrap();
        file.sync_all().unwrap();
    }

    pub fn get_default_identity_config(&self) -> Option<&IdentityConfig> {
        if let Some(identity) = &self.default_identity() {
            let config = self
                .identity_configs()
                .iter()
                .find(|c| &c.identity == identity)
                .unwrap();
            Some(config)
        } else {
            None
        }
    }

    pub fn name_exists(&self, nickname: &str) -> bool {
        for name in self.identity_configs().iter().map(|c| &c.nickname) {
            if name.as_ref() == Some(&nickname.to_string()) {
                return true;
            }
        }
        false
    }

    pub fn get_identity_config_by_name(&self, name: &str) -> Option<&IdentityConfig> {
        self.identity_configs()
            .iter()
            .find(|c| c.nickname.as_ref() == Some(&name.to_string()))
    }

    pub fn get_identity_config_by_identity(&self, identity: &str) -> Option<&IdentityConfig> {
        self.identity_configs().iter().find(|c| c.identity == identity)
    }

    pub fn get_identity_config_by_identity_mut(&mut self, identity: &str) -> Option<&mut IdentityConfig> {
        self.identity_configs_mut().iter_mut().find(|c| c.identity == identity)
    }

    /// Converts some given `identity_or_name` into an identity.
    ///
    /// If `identity_or_name` is `None` then `None` is returned. If `identity_or_name` is `Some`,
    /// then if its an identity then its just returned. If its not an identity it is assumed to be
    /// a name and it is looked up as an identity nickname. If the identity exists it is returned,
    /// otherwise we panic.
    pub fn resolve_name_to_identity(&self, identity_or_name: Option<&str>) -> Option<String> {
        identity_or_name
            .map(|identity_or_name| {
                if is_hex_identity(identity_or_name) {
                    &self
                        .identity_configs()
                        .iter()
                        .find(|c| c.identity == *identity_or_name)
                        .unwrap_or_else(|| panic!("No such identity: {}", identity_or_name))
                        .identity
                } else {
                    &self
                        .identity_configs()
                        .iter()
                        .find(|c| c.nickname == Some(identity_or_name.to_string()))
                        .unwrap_or_else(|| panic!("No such identity: {}", identity_or_name))
                        .identity
                }
            })
            .cloned()
    }

    /// Converts some given `identity_or_name` into a mutable `IdentityConfig`.
    ///
    /// # Returns
    /// * `None` - If an identity config with the given `identity_or_name` does not exist.
    /// * `Some` - A mutable reference to the `IdentityConfig` with the given `identity_or_name`.
    pub fn get_identity_config_mut(&mut self, identity_or_name: &str) -> Option<&mut IdentityConfig> {
        if is_hex_identity(identity_or_name) {
            self.get_identity_config_by_identity_mut(identity_or_name)
        } else {
            self.identity_configs_mut()
                .iter_mut()
                .find(|c| c.nickname.as_deref() == Some(identity_or_name))
        }
    }

    pub fn delete_identity_config_by_name(&mut self, name: &str) -> Option<IdentityConfig> {
        let index = self
            .home
            .identity_configs
            .as_ref()
            .unwrap()
            .iter()
            .position(|c| c.nickname.as_deref() == Some(name));
        if let Some(index) = index {
            Some(self.home.identity_configs.as_mut().unwrap().remove(index))
        } else {
            None
        }
    }

    pub fn delete_identity_config_by_identity(&mut self, identity: &str) -> Option<IdentityConfig> {
        let index = self
            .home
            .identity_configs
            .as_ref()
            .unwrap()
            .iter()
            .position(|c| c.identity == identity);
        if let Some(index) = index {
            Some(self.home.identity_configs.as_mut().unwrap().remove(index))
        } else {
            None
        }
    }

    /// Deletes all stored identity configs. This function does not save the config after removing
    /// all configs.
    pub fn delete_all_identity_configs(&mut self) {
        self.home.identity_configs = Some(vec![]);
        self.home.default_identity = None;
    }

    pub fn update_default_identity(&mut self) {
        if let Some(default_identity) = &self.home.default_identity {
            if self
                .identity_configs()
                .iter()
                .map(|c| &c.identity)
                .any(|i| i == default_identity)
            {
                return;
            }
        }
        self.home.default_identity = self.identity_configs().first().map(|c| c.identity.clone())
    }

    pub fn get_host_url(&self) -> String {
        format!("{}://{}", self.protocol(), self.host())
    }
}
