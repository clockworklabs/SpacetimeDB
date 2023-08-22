use crate::util::{contains_protocol, host_or_url_to_host_and_protocol, is_hex_identity};
use anyhow::Context;
use jsonwebtoken::DecodingKey;
use serde::{Deserialize, Serialize};
use spacetimedb::auth::identity::decode_token;
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

impl IdentityConfig {
    fn nick_or_identity(&self) -> &str {
        if let Some(nick) = &self.nickname {
            nick
        } else {
            &self.identity
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerConfig {
    pub nickname: Option<String>,
    pub host: String,
    pub protocol: String,
    pub default_identity: Option<String>,
    pub ecdsa_public_key: Option<String>,
}

impl ServerConfig {
    fn nick_or_host(&self) -> &str {
        if let Some(nick) = &self.nickname {
            nick
        } else {
            &self.host
        }
    }
    pub fn get_host_url(&self) -> String {
        format!("{}://{}", self.protocol, self.host)
    }

    pub fn set_default_identity(&mut self, default_identity: String) {
        self.default_identity = Some(default_identity);
        // TODO: verify the identity exists and its token conforms to the server's `ecdsa_public_key`
    }

    pub fn nick_or_host_or_url_is(&self, name: &str) -> bool {
        self.nickname.as_deref() == Some(name) || self.host == name || {
            let (host, _) = host_or_url_to_host_and_protocol(name);
            self.host == host
        }
    }

    fn default_identity(&self) -> anyhow::Result<&str> {
        self.default_identity
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No default identity for server: {}", self.nick_or_host()))
    }

    fn assert_identity_applies(&self, id: &IdentityConfig) -> anyhow::Result<()> {
        if let Some(fingerprint) = &self.ecdsa_public_key {
            let decoder = DecodingKey::from_ec_pem(fingerprint.as_bytes()).with_context(|| {
                format!(
                    "Verifying tokens using saved fingerprint from server: {}",
                    self.nick_or_host(),
                )
            })?;
            decode_token(&decoder, &id.token).map_err(|_| {
                anyhow::anyhow!(
                    "Identity {} is not valid for server {}",
                    id.nick_or_identity(),
                    self.nick_or_host()
                )
            })?;
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RawConfig {
    default_server: Option<String>,
    identity_configs: Option<Vec<IdentityConfig>>,
    server_configs: Option<Vec<ServerConfig>>,
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

const DEFAULT_HOST: &str = "testnet.spacetimedb.com";
const DEFAULT_PROTOCOL: &str = "https";

impl RawConfig {
    fn find_server(&self, name_or_host: &str) -> anyhow::Result<&ServerConfig> {
        if let Some(server_configs) = &self.server_configs {
            for cfg in server_configs {
                if cfg.nickname.as_deref() == Some(name_or_host) || cfg.host == name_or_host {
                    return Ok(cfg);
                }
            }
        }
        Err(anyhow::anyhow!("No such saved server configuration: {}", name_or_host,))
    }

    fn find_server_mut(&mut self, name_or_host: &str) -> anyhow::Result<&mut ServerConfig> {
        if let Some(server_configs) = &mut self.server_configs {
            for cfg in server_configs {
                if cfg.nickname.as_deref() == Some(name_or_host) || cfg.host == name_or_host {
                    return Ok(cfg);
                }
            }
        }
        Err(anyhow::anyhow!("No such saved server configuration: {}", name_or_host,))
    }

    fn default_server(&self) -> anyhow::Result<&ServerConfig> {
        if let Some(default_server) = self.default_server.as_ref() {
            self.find_server(default_server)
                .with_context(|| "Finding server configuration for default server")
        } else {
            Err(anyhow::anyhow!("No default server configuration"))
        }
    }

    fn default_server_mut(&mut self) -> anyhow::Result<&mut ServerConfig> {
        if let Some(default_server) = self.default_server.as_ref() {
            let default = default_server.to_string();
            self.find_server_mut(&default)
                .with_context(|| "Finding server configuration for default server")
        } else {
            Err(anyhow::anyhow!("No default server configuration"))
        }
    }

    fn find_identity_config(&self, identity: &str) -> anyhow::Result<&IdentityConfig> {
        if let Some(identity_configs) = &self.identity_configs {
            for cfg in identity_configs {
                if cfg.nickname.as_deref() == Some(identity) || cfg.identity == identity {
                    return Ok(cfg);
                }
            }
        }
        Err(anyhow::anyhow!("No such saved identity configuration: {}", identity,))
    }

    fn add_server(
        &mut self,
        host: String,
        protocol: String,
        ecdsa_public_key: Option<String>,
        nickname: Option<String>,
    ) -> anyhow::Result<()> {
        if let Some(nickname) = &nickname {
            if let Ok(cfg) = self.find_server(nickname) {
                anyhow::bail!(
                    "Server nickname {} already in use: {}://{}",
                    nickname,
                    cfg.protocol,
                    cfg.host,
                );
            }
        }

        if let Ok(cfg) = self.find_server(&host) {
            if let Some(nick) = &cfg.nickname {
                if nick == &host {
                    anyhow::bail!("Server host name is ambiguous with existing server nickname: {}", nick,);
                }
            }
            anyhow::bail!("Server already configured for host: {}", host,);
        }

        if self.server_configs.is_none() {
            self.server_configs = Some(Vec::new());
        }

        let server_configs = self.server_configs.as_mut().unwrap();

        server_configs.push(ServerConfig {
            nickname,
            host,
            protocol,
            ecdsa_public_key,
            default_identity: None,
        });
        Ok(())
    }

    fn host(&self, server: &str) -> anyhow::Result<&str> {
        self.find_server(server)
            .map(|cfg| cfg.host.as_ref())
            .with_context(|| "Finding hostname for server")
    }

    fn default_host(&self) -> anyhow::Result<&str> {
        self.default_server()
            .with_context(|| "Finding hostname for default server")
            .map(|cfg| cfg.host.as_ref())
    }

    fn protocol(&self, server: &str) -> anyhow::Result<&str> {
        self.find_server(server).map(|cfg| cfg.protocol.as_ref())
    }

    fn default_protocol(&self) -> anyhow::Result<&str> {
        self.default_server()
            .with_context(|| "Finding protocol for default server")
            .map(|cfg| cfg.protocol.as_ref())
    }

    fn default_identity(&self, server: &str) -> anyhow::Result<&str> {
        self.find_server(server).and_then(ServerConfig::default_identity)
    }

    fn default_server_default_identity(&self) -> anyhow::Result<&str> {
        self.default_server().and_then(ServerConfig::default_identity)
    }

    fn assert_identity_matches_server(&self, server: &str, identity: &str) -> anyhow::Result<()> {
        let ident = self
            .find_identity_config(identity)
            .with_context(|| format!("Verifying that identity {} applies to server {}", identity, server))?;
        let server_cfg = self
            .find_server(server)
            .with_context(|| format!("Verifying that identity {} applies to server {}", identity, server))?;
        server_cfg.assert_identity_applies(ident)
    }

    fn set_server_default_identity(&mut self, server: &str, default_identity: String) -> anyhow::Result<()> {
        self.assert_identity_matches_server(server, &default_identity)?;
        let cfg = self.find_server_mut(server)?;
        // TODO: create the server config if it doesn't already exist
        // TODO: fetch the server's fingerprint to check if it has changed
        cfg.default_identity = Some(default_identity);
        Ok(())
    }

    fn set_default_server_default_identity(&mut self, default_identity: String) -> anyhow::Result<()> {
        if let Some(default_server) = &self.default_server {
            self.assert_identity_matches_server(default_server, &default_identity)?;

            // Unfortunate clone,
            // because `set_server_default_identity` needs a unique ref to `self`.
            let def = default_server.to_string();
            self.set_server_default_identity(&def, default_identity)
        } else {
            Err(anyhow::anyhow!("No default server configuration"))
        }
    }

    fn unset_all_default_identities(&mut self) {
        if let Some(server_configs) = &mut self.server_configs {
            for cfg in server_configs {
                cfg.default_identity = None;
            }
        }
    }

    fn update_all_default_identities(&mut self) {
        if let Some(servers) = &mut self.server_configs {
            for server in servers.iter_mut() {
                if let Some(default_identity) = &server.default_identity {
                    if self
                        .identity_configs
                        .iter()
                        .flat_map(|cfgs| cfgs.iter())
                        .any(|cfg| &cfg.identity == default_identity)
                    {
                        server.default_identity = None;
                        println!(
                            "Unsetting removed default identity for server: {}",
                            server.nick_or_host(),
                        );
                        // TODO: Find an appropriate identity and set it as the default?
                    }
                }
            }
        }
    }

    fn set_default_identity_if_unset(&mut self, server: &str, identity: &str) -> anyhow::Result<()> {
        let cfg = self.find_server_mut(server)?;
        if cfg.default_identity.is_none() {
            cfg.default_identity = Some(identity.to_string());
        }
        Ok(())
    }

    fn default_server_set_default_identity_if_unset(&mut self, identity: &str) -> anyhow::Result<()> {
        let cfg = self.default_server_mut()?;
        if cfg.default_identity.is_none() {
            cfg.default_identity = Some(identity.to_string());
        }
        Ok(())
    }

    fn set_default_server(&mut self, server: &str) -> anyhow::Result<()> {
        self.find_server(server)
            .with_context(|| "Finding server configuration to set default server")?;
        self.default_server = Some(server.to_string());
        Ok(())
    }

    fn remove_server(&mut self, server: &str, delete_identities: bool) -> anyhow::Result<()> {
        if let Some(server_configs) = &mut self.server_configs {
            if let Some(idx) = server_configs.iter().position(|cfg| cfg.nick_or_host_or_url_is(server)) {
                let cfg = server_configs.remove(idx);
                if delete_identities {
                    let fingerprint = cfg.ecdsa_public_key.ok_or_else(|| {
                        anyhow::anyhow!("Cannot delete identities for server without fingerprint: {}", server)
                    })?;

                    let decoder = DecodingKey::from_ec_pem(fingerprint.as_bytes())
                        .with_context(|| format!("Verifying tokens using saved fingerprint from server: {}", server))?;

                    if let Some(identity_configs) = self.identity_configs.take() {
                        let identity_configs = identity_configs
                            .into_iter()
                            .filter(|cfg| decode_token(&decoder, &cfg.token).is_err())
                            .collect::<Vec<_>>();
                        self.identity_configs = if identity_configs.is_empty() {
                            None
                        } else {
                            Some(identity_configs)
                        };
                    }
                }
                if server_configs.is_empty() {
                    self.server_configs = None;
                }
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("No such saved server configuration: {}", server))
    }

    fn server_fingerprint(&self, server: &str) -> anyhow::Result<Option<&str>> {
        self.find_server(server)
            .with_context(|| "Looking up fingerprint for server configuration")
            .map(|cfg| cfg.ecdsa_public_key.as_deref())
    }

    fn default_server_fingerprint(&self) -> anyhow::Result<Option<&str>> {
        if let Some(server) = &self.default_server {
            self.server_fingerprint(server)
        } else {
            Err(anyhow::anyhow!("No default server configuration"))
        }
    }

    fn set_server_fingerprint(&mut self, server: &str, ecdsa_public_key: String) -> anyhow::Result<()> {
        let cfg = self.find_server_mut(server)?;
        cfg.ecdsa_public_key = Some(ecdsa_public_key);
        Ok(())
    }

    fn set_default_server_fingerprint(&mut self, ecdsa_public_key: String) -> anyhow::Result<()> {
        let cfg = self.default_server_mut()?;
        cfg.ecdsa_public_key = Some(ecdsa_public_key);
        Ok(())
    }
}

impl Config {
    pub fn default_server_name(&self) -> Option<&str> {
        self.proj
            .default_server
            .as_deref()
            .or(self.home.default_server.as_deref())
    }

    pub fn add_server(
        &mut self,
        host: String,
        protocol: String,
        ecdsa_public_key: Option<String>,
        nickname: Option<String>,
        project: bool,
    ) -> anyhow::Result<()> {
        if project {
            self.proj.add_server(host, protocol, ecdsa_public_key, nickname)
        } else {
            self.home.add_server(host, protocol, ecdsa_public_key, nickname)
        }
    }

    pub fn set_default_server(&mut self, nickname_or_host_or_url: &str, project: bool) -> anyhow::Result<()> {
        let (host, _) = host_or_url_to_host_and_protocol(nickname_or_host_or_url);
        if project {
            self.proj.set_default_server(host)
        } else {
            self.home.set_default_server(host)
        }
    }

    pub fn remove_server(
        &mut self,
        nickname_or_host_or_url: &str,
        project: bool,
        delete_identities: bool,
    ) -> anyhow::Result<()> {
        let (host, _) = host_or_url_to_host_and_protocol(nickname_or_host_or_url);
        if project {
            self.proj.remove_server(host, delete_identities)
        } else {
            self.home.remove_server(host, delete_identities)
        }
    }

    pub fn get_host_url(&self, server: Option<&str>) -> anyhow::Result<String> {
        Ok(format!("{}://{}", self.protocol(server)?, self.host(server)?))
    }

    pub fn host<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            if contains_protocol(server) {
                Ok(host_or_url_to_host_and_protocol(server).0)
            } else {
                self.proj.host(server).or_else(|_| self.home.host(server))
            }
        } else {
            self.proj
                .default_host()
                .or_else(|_| self.home.default_host())
                .or(Ok(DEFAULT_HOST))
        }
    }

    pub fn protocol<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            if contains_protocol(server) {
                Ok(host_or_url_to_host_and_protocol(server).1.unwrap())
            } else {
                self.proj.protocol(server).or_else(|_| self.home.protocol(server))
            }
        } else {
            self.proj
                .default_protocol()
                .or_else(|_| self.home.default_protocol())
                .or(Ok(DEFAULT_PROTOCOL))
        }
    }

    pub fn default_identity(&self, server: Option<&str>) -> anyhow::Result<&str> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.proj
                .default_identity(host)
                .or_else(|_| self.home.default_identity(host))
        } else {
            self.proj
                .default_server_default_identity()
                .or_else(|_| self.home.default_server_default_identity())
        }
    }

    pub fn set_default_identity(&mut self, default_identity: String, server: Option<&str>) -> anyhow::Result<()> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.proj
                .set_server_default_identity(host, default_identity.clone())
                .or_else(|_| self.home.set_server_default_identity(host, default_identity))
        } else {
            self.proj
                .set_default_server_default_identity(default_identity.clone())
                .or_else(|_| self.home.set_default_server_default_identity(default_identity))
        }
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

    pub fn identity_configs(&self) -> &Vec<IdentityConfig> {
        self.home.identity_configs.as_ref().unwrap()
    }

    pub fn identity_configs_mut(&mut self) -> &mut Vec<IdentityConfig> {
        self.home.identity_configs.get_or_insert(vec![])
    }

    pub fn server_configs(&self) -> &[ServerConfig] {
        self.home.server_configs.as_deref().unwrap_or(&[])
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

    fn load_raw(config_dir: PathBuf, is_project: bool) -> RawConfig {
        // If a config file overload has been specified, use that instead
        if !is_project {
            if let Some(config_path) = std::env::var_os("SPACETIME_CONFIG_FILE") {
                return Self::load_from_file(config_path.as_ref());
            }
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
        let mut home_config = Self::load_raw(home_dir.join(HOME_CONFIG_DIR), false);

        // Ensure there is always an identity config. Simplifies other code.
        home_config.identity_configs.get_or_insert(vec![]);

        // TODO(cloutiertyler): For now we're checking for a spacetime.toml file
        // in the current directory. Eventually this should really be that we
        // search parent directories above the current directory to find
        // spacetime.toml files like a .gitignore file
        let cur_dir = std::env::current_dir().expect("No current working directory!");
        let cur_config = Self::load_raw(cur_dir, true);

        Self {
            home: home_config,
            proj: cur_config,
        }
    }

    pub fn save(&self) {
        let config_path = if let Some(config_path) = std::env::var_os("SPACETIME_CONFIG_FILE") {
            PathBuf::from(&config_path)
        } else {
            let home_dir = dirs::home_dir().unwrap();
            let config_dir = home_dir.join(HOME_CONFIG_DIR);
            if !config_dir.exists() {
                fs::create_dir_all(&config_dir).unwrap();
            }

            let config_filename = Self::find_config_filename(&config_dir).unwrap_or(CONFIG_FILENAME);
            config_dir.join(config_filename)
        };

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

    pub fn get_default_identity_config(&self, server: Option<&str>) -> anyhow::Result<&IdentityConfig> {
        self.default_identity(server).and_then(|identity| {
            self.identity_configs()
                .iter()
                .find(|c| c.identity == identity)
                .ok_or_else(|| anyhow::anyhow!("No saved configuration for identity: {}", identity))
        })
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
    pub fn resolve_name_to_identity(&self, identity_or_name: Option<&str>) -> anyhow::Result<Option<String>> {
        Ok(if let Some(identity_or_name) = identity_or_name {
            let x = if is_hex_identity(identity_or_name) {
                &self
                    .identity_configs()
                    .iter()
                    .find(|c| c.identity == *identity_or_name)
                    .ok_or_else(|| anyhow::anyhow!("No such identity: {}", identity_or_name))?
                    .identity
            } else {
                &self
                    .identity_configs()
                    .iter()
                    .find(|c| c.nickname == Some(identity_or_name.to_string()))
                    .ok_or_else(|| anyhow::anyhow!("No such identity: {}", identity_or_name))?
                    .identity
            };
            Some(x.clone())
        } else {
            None
        })
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
        self.home.unset_all_default_identities();
    }

    pub fn update_all_default_identities(&mut self) {
        self.home.update_all_default_identities();
    }

    pub fn set_default_identity_if_unset(&mut self, server: Option<&str>, identity: &str) -> anyhow::Result<()> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.proj
                .set_default_identity_if_unset(host, identity)
                .or_else(|_| self.home.set_default_identity_if_unset(host, identity))
        } else {
            self.proj
                .default_server_set_default_identity_if_unset(identity)
                .or_else(|_| self.home.default_server_set_default_identity_if_unset(identity))
        }
    }

    pub fn server_decoding_key(&self, server: Option<&str>) -> anyhow::Result<DecodingKey> {
        self.server_fingerprint(server).and_then(|fing| {
            if let Some(fing) = fing {
                DecodingKey::from_ec_pem(fing.as_bytes())
                    .with_context(|| "Parsing server fingerprint as ECDSA public key")
            } else {
                Err(anyhow::anyhow!(
                    "No fingerprint saved for server: {}",
                    self.server_nick_or_host(server)?,
                ))
            }
        })
    }

    pub fn server_nick_or_host<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            Ok(host)
        } else {
            self.proj
                .default_server()
                .or_else(|_| self.home.default_server())
                .map(ServerConfig::nick_or_host)
        }
    }

    pub fn server_fingerprint(&self, server: Option<&str>) -> anyhow::Result<Option<&str>> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.proj
                .server_fingerprint(host)
                .or_else(|_| self.home.server_fingerprint(host))
        } else {
            self.proj
                .default_server_fingerprint()
                .or_else(|_| self.home.default_server_fingerprint())
        }
    }

    pub fn set_server_fingerprint(&mut self, server: Option<&str>, new_fingerprint: String) -> anyhow::Result<()> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.home.set_server_fingerprint(host, new_fingerprint)
        } else {
            self.home.set_default_server_fingerprint(new_fingerprint)
        }
    }
}
