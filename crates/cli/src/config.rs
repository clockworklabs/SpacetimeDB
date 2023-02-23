use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
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
    pub fn host(&self) -> &str {
        self.proj
            .host
            .as_ref()
            .or(self.home.host.as_ref())
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_HOST)
    }

    pub fn set_host(&mut self, host: &str) {
        self.home.host = Some(host.to_string());
    }

    pub fn protocol(&self) -> &str {
        self.proj
            .protocol
            .as_ref()
            .or(self.home.protocol.as_ref())
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_PROTOCOL)
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
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).unwrap();
        }

        let config_filename = Self::find_config_filename(&config_dir);
        let Some(config_filename) = config_filename else {
            // Return an empty raw config without creating a file.
            return toml::from_str("").unwrap();
        };

        let config_path = config_dir.join(config_filename);
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
