use std::{fs, io::{Read, Write}};
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize)]
pub struct IdentityConfig {
    pub nickname: Option<String>,
    pub identity: String,
    pub email: Option<String>,
    pub token: String,
}

#[derive(Deserialize)]
pub struct RawConfig {
    host: Option<String>,
    default_identity: Option<String>,
    identity_configs: Option<Vec<IdentityConfig>>,
}

#[derive(Serialize)]
pub struct Config {
    pub host: String,
    pub default_identity: Option<String>,
    pub identity_configs: Vec<IdentityConfig>,
}

const CONFIG_DIR: &str = ".stdb";
const CONFIG_FILENAME: &str = "config.toml";

impl Config {
    pub fn load() -> Self {
        let home_dir = dirs::home_dir().unwrap();
        let config_dir = home_dir.join(CONFIG_DIR);
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).unwrap();
        }

        let config_path = config_dir.join(CONFIG_FILENAME);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(config_path)
            .unwrap();

        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        let config: RawConfig = toml::from_str(&text).unwrap();

        Self::from_raw(config)
    }

    fn from_raw(raw: RawConfig) -> Self {
        Self {
            identity_configs: raw.identity_configs.unwrap_or(Vec::new()),
            host: raw.host.unwrap_or("localhost:3000".into()),
            default_identity: raw.default_identity,
        }
    }

    pub fn save(&self) {
        let home_dir = dirs::home_dir().unwrap();
        let config_dir = home_dir.join(CONFIG_DIR);
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).unwrap();
        }

        let config_path = config_dir.join(CONFIG_FILENAME);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(config_path)
            .unwrap();
        
        let str = toml::to_string_pretty(self).unwrap();

        file.set_len(0).unwrap();
        file.write_all(str.as_bytes()).unwrap();
        file.sync_all().unwrap();
    }

    pub fn get_default_identity_config(&self) -> Option<&IdentityConfig> {
        if let Some(identity) = &self.default_identity {
            let config = self.identity_configs.iter().find(|c| &c.identity == identity).unwrap();
            Some(config)
        } else {
            None
        }
    }

    pub fn name_exists(&self, nickname: &str) -> bool {
        for name in self.identity_configs.iter().map(|c| &c.nickname) {
            if name.as_ref() == Some(&nickname.to_string()) {
                return true;
            }
        }
        return false;
    }

    pub fn get_identity_config_by_name(&self, name: &str) -> Option<&IdentityConfig> {
        self.identity_configs.iter().find(|c| c.nickname.as_ref() == Some(&name.to_string()))
    }
    
    pub fn get_identity_config_by_identity(&self, identity: &str) -> Option<&IdentityConfig> {
        self.identity_configs.iter().find(|c| &c.identity == identity)
    }
    
    pub fn get_identity_config_by_identity_mut(&mut self, identity: &str) -> Option<&mut IdentityConfig> {
        self.identity_configs.iter_mut().find(|c| &c.identity == identity)
    }

    pub fn update_default_identity(&mut self) {
        if let Some(default_identity) = &self.default_identity {
            if self.identity_configs.iter().map(|c| &c.identity).any(|i| i == default_identity) {
                return;
            }
        }
        self.default_identity = self.identity_configs.first().map(|c| c.identity.clone())
    }

}