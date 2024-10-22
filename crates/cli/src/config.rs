use crate::util::{contains_protocol, host_or_url_to_host_and_protocol};
use anyhow::Context;
use jsonwebtoken::DecodingKey;
use serde::{Deserialize, Serialize};
use spacetimedb_fs_utils::{atomic_write, create_parent_dir};
use std::{
    fs,
    path::{Path, PathBuf},
};

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
        self.default_identity.as_deref().ok_or_else(|| {
            let server = self.nick_or_host();
            anyhow::anyhow!(
                "No default identity for server: {server}
Set the default identity with:
\tspacetime identity set-default -s {server} <identity>
Or initialize a default identity with:
\tspacetime identity init-default -s {server}"
            )
        })
    }
}

#[derive(Default, Deserialize, Serialize, Debug, Clone)]
pub struct RawConfig {
    default_server: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    server_configs: Vec<ServerConfig>,
    web_session_id: Option<String>,
    spacetimedb_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    home: RawConfig,
}

const HOME_CONFIG_DIR: &str = ".spacetime";
const CONFIG_FILENAME: &str = "config.toml";
const SPACETIME_FILENAME: &str = "spacetime.toml";
const DOT_SPACETIME_FILENAME: &str = ".spacetime.toml";

const NO_DEFAULT_SERVER_ERROR_MESSAGE: &str = "No default server configuration.
Set an existing server as the default with:
\tspacetime server set-default <server>
Or add a new server which will become the default:
\tspacetime server add <url> --default";

fn no_such_server_error(server: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "No such saved server configuration: {server}
Add a new server configuration with:
\tspacetime server add <url>",
    )
}

fn hanging_default_server_context(server: &str) -> String {
    format!("Default server does not refer to a saved server configuration: {server}")
}

impl RawConfig {
    fn new_with_localhost() -> Self {
        let local = ServerConfig {
            default_identity: None,
            host: "127.0.0.1:3000".to_string(),
            protocol: "http".to_string(),
            nickname: Some("local".to_string()),
            ecdsa_public_key: None,
        };
        let testnet = ServerConfig {
            default_identity: None,
            host: "testnet.spacetimedb.com".to_string(),
            protocol: "https".to_string(),
            nickname: Some("testnet".to_string()),
            ecdsa_public_key: None,
        };
        RawConfig {
            default_server: local.nickname.clone(),
            server_configs: vec![local, testnet],
            web_session_id: None,
            spacetimedb_token: None,
        }
    }

    fn find_server(&self, name_or_host: &str) -> anyhow::Result<&ServerConfig> {
        for cfg in &self.server_configs {
            if cfg.nickname.as_deref() == Some(name_or_host) || cfg.host == name_or_host {
                return Ok(cfg);
            }
        }
        Err(no_such_server_error(name_or_host))
    }

    fn find_server_mut(&mut self, name_or_host: &str) -> anyhow::Result<&mut ServerConfig> {
        for cfg in &mut self.server_configs {
            if cfg.nickname.as_deref() == Some(name_or_host) || cfg.host == name_or_host {
                return Ok(cfg);
            }
        }
        Err(no_such_server_error(name_or_host))
    }

    fn default_server(&self) -> anyhow::Result<&ServerConfig> {
        if let Some(default_server) = self.default_server.as_ref() {
            self.find_server(default_server)
                .with_context(|| hanging_default_server_context(default_server))
        } else {
            Err(anyhow::anyhow!(NO_DEFAULT_SERVER_ERROR_MESSAGE))
        }
    }

    fn default_server_mut(&mut self) -> anyhow::Result<&mut ServerConfig> {
        if let Some(default_server) = self.default_server.as_ref() {
            let default = default_server.to_string();
            self.find_server_mut(&default)
                .with_context(|| hanging_default_server_context(&default))
        } else {
            Err(anyhow::anyhow!(NO_DEFAULT_SERVER_ERROR_MESSAGE))
        }
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
                    anyhow::bail!("Server host name is ambiguous with existing server nickname: {}", nick);
                }
            }
            anyhow::bail!("Server already configured for host: {}", host);
        }

        self.server_configs.push(ServerConfig {
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
            .with_context(|| format!("Cannot find hostname for unknown server: {server}"))
    }

    fn default_host(&self) -> anyhow::Result<&str> {
        self.default_server()
            .with_context(|| "Cannot find hostname for default server")
            .map(|cfg| cfg.host.as_ref())
    }

    fn protocol(&self, server: &str) -> anyhow::Result<&str> {
        self.find_server(server).map(|cfg| cfg.protocol.as_ref())
    }

    fn default_protocol(&self) -> anyhow::Result<&str> {
        self.default_server()
            .with_context(|| "Cannot find protocol for default server")
            .map(|cfg| cfg.protocol.as_ref())
    }

    fn default_identity(&self, server: &str) -> anyhow::Result<&str> {
        self.find_server(server).and_then(ServerConfig::default_identity)
    }

    fn default_server_default_identity(&self) -> anyhow::Result<&str> {
        self.default_server().and_then(ServerConfig::default_identity)
    }

    fn set_server_default_identity(&mut self, server: &str, default_identity: String) -> anyhow::Result<()> {
        let cfg = self.find_server_mut(server)?;
        // TODO: create the server config if it doesn't already exist
        // TODO: fetch the server's fingerprint to check if it has changed
        cfg.default_identity = Some(default_identity);
        Ok(())
    }

    fn set_default_server_default_identity(&mut self, default_identity: String) -> anyhow::Result<()> {
        if let Some(default_server) = &self.default_server {
            // Unfortunate clone,
            // because `set_server_default_identity` needs a unique ref to `self`.
            let def = default_server.to_string();
            self.set_server_default_identity(&def, default_identity)
        } else {
            Err(anyhow::anyhow!(NO_DEFAULT_SERVER_ERROR_MESSAGE))
        }
    }

    fn set_default_server(&mut self, server: &str) -> anyhow::Result<()> {
        // Check that such a server exists before setting the default.
        self.find_server(server)
            .with_context(|| format!("Cannot set default server to unknown server {server}"))?;

        self.default_server = Some(server.to_string());

        Ok(())
    }

    /// Implements `spacetime server remove`.
    fn remove_server(&mut self, server: &str) -> anyhow::Result<()> {
        // Have to find the server config manually instead of doing `find_server_mut`
        // because we need to mutably borrow multiple components of `self`.
        if let Some(idx) = self
            .server_configs
            .iter()
            .position(|cfg| cfg.nick_or_host_or_url_is(server))
        {
            // Actually remove the config.
            let cfg = self.server_configs.remove(idx);

            // If we're removing the default server,
            // unset the default server.
            if let Some(default_server) = &self.default_server {
                if cfg.nick_or_host_or_url_is(default_server) {
                    self.default_server = None;
                }
            }

            return Ok(());
        }
        Err(no_such_server_error(server))
    }

    /// Return the ECDSA public key in PEM format for the server named by `server`.
    ///
    /// Returns an `Err` if there is no such server configuration.
    /// Returns `None` if the server configuration exists, but does not have a fingerprint saved.
    fn server_fingerprint(&self, server: &str) -> anyhow::Result<Option<&str>> {
        self.find_server(server)
            .with_context(|| {
                format!(
                    "No saved fingerprint for server: {server}
Fetch the server's fingerprint with:
\tspacetime server fingerprint -s {server}"
                )
            })
            .map(|cfg| cfg.ecdsa_public_key.as_deref())
    }

    /// Return the ECDSA public key in PEM format for the default server.
    ///
    /// Returns an `Err` if there is no default server configuration.
    /// Returns `None` if the server configuration exists, but does not have a fingerprint saved.
    fn default_server_fingerprint(&self) -> anyhow::Result<Option<&str>> {
        if let Some(server) = &self.default_server {
            self.server_fingerprint(server)
        } else {
            Err(anyhow::anyhow!(NO_DEFAULT_SERVER_ERROR_MESSAGE))
        }
    }

    /// Store the fingerprint for the server named `server`.
    ///
    /// Returns an `Err` if no such server configuration exists.
    /// On success, any existing fingerprint is dropped.
    fn set_server_fingerprint(&mut self, server: &str, ecdsa_public_key: String) -> anyhow::Result<()> {
        let cfg = self.find_server_mut(server)?;
        cfg.ecdsa_public_key = Some(ecdsa_public_key);
        Ok(())
    }

    /// Store the fingerprint for the default server.
    ///
    /// Returns an `Err` if no default server configuration exists.
    /// On success, any existing fingerprint is dropped.
    fn set_default_server_fingerprint(&mut self, ecdsa_public_key: String) -> anyhow::Result<()> {
        let cfg = self.default_server_mut()?;
        cfg.ecdsa_public_key = Some(ecdsa_public_key);
        Ok(())
    }

    /// Edit a saved server configuration.
    ///
    /// Implements `spacetime server edit`.
    ///
    /// Returns `Err` if no such server exists.
    /// On success, returns `(old_nickname, old_host, hold_protocol)`,
    /// with `Some` for each field that was changed.
    pub fn edit_server(
        &mut self,
        server: &str,
        new_nickname: Option<&str>,
        new_host: Option<&str>,
        new_protocol: Option<&str>,
    ) -> anyhow::Result<(Option<String>, Option<String>, Option<String>)> {
        // Check if the new nickname or host name would introduce ambiguities between
        // server configurations.
        if let Some(new_nick) = new_nickname {
            if let Ok(other_server) = self.find_server(new_nick) {
                anyhow::bail!(
                    "Nickname {} conflicts with saved configuration for server {}: {}://{}",
                    new_nick,
                    other_server.nick_or_host(),
                    other_server.protocol,
                    other_server.host
                );
            }
        }
        if let Some(new_host) = new_host {
            if let Ok(other_server) = self.find_server(new_host) {
                anyhow::bail!(
                    "Host {} conflicts with saved configuration for server {}: {}://{}",
                    new_host,
                    other_server.nick_or_host(),
                    other_server.protocol,
                    other_server.host
                );
            }
        }

        let cfg = self.find_server_mut(server)?;
        let old_nickname = if let Some(new_nickname) = new_nickname {
            std::mem::replace(&mut cfg.nickname, Some(new_nickname.to_string()))
        } else {
            None
        };
        let old_host = if let Some(new_host) = new_host {
            Some(std::mem::replace(&mut cfg.host, new_host.to_string()))
        } else {
            None
        };
        let old_protocol = if let Some(new_protocol) = new_protocol {
            Some(std::mem::replace(&mut cfg.protocol, new_protocol.to_string()))
        } else {
            None
        };

        // If the server we edited was the default server,
        // and we changed the identifier stored in the `default_server` field,
        // update that field.
        if let Some(default_server) = &mut self.default_server {
            if let Some(old_host) = &old_host {
                if default_server == old_host {
                    *default_server = new_host.unwrap().to_string();
                }
            } else if let Some(old_nick) = &old_nickname {
                if default_server == old_nick {
                    *default_server = new_nickname.unwrap().to_string();
                }
            }
        }

        Ok((old_nickname, old_host, old_protocol))
    }

    pub fn delete_server_fingerprint(&mut self, server: &str) -> anyhow::Result<()> {
        let cfg = self.find_server_mut(server)?;
        cfg.ecdsa_public_key = None;
        Ok(())
    }

    pub fn delete_default_server_fingerprint(&mut self) -> anyhow::Result<()> {
        let cfg = self.default_server_mut()?;
        cfg.ecdsa_public_key = None;
        Ok(())
    }

    pub fn set_web_session_id(&mut self, token: String) {
        self.web_session_id = Some(token);
    }

    pub fn set_spacetimedb_token(&mut self, token: String) {
        self.spacetimedb_token = Some(token);
    }
}

impl Config {
    pub fn default_server_name(&self) -> Option<&str> {
        self.home.default_server.as_deref()
    }

    /// Add a `ServerConfig` to the home configuration.
    ///
    /// Returns an `Err` on name conflict,
    /// i.e. if a `ServerConfig` with the `nickname` or `host` already exists.
    ///
    /// Callers should call `Config::save` afterwards
    /// to ensure modifications are persisted to disk.
    pub fn add_server(
        &mut self,
        host: String,
        protocol: String,
        ecdsa_public_key: Option<String>,
        nickname: Option<String>,
    ) -> anyhow::Result<()> {
        self.home.add_server(host, protocol, ecdsa_public_key, nickname)
    }

    /// Set the default server in the home configuration.
    ///
    /// Returns an `Err` if `nickname_or_host_or_url`
    /// does not refer to an existing `ServerConfig`
    /// in the home configuration.
    ///
    /// Callers should call `Config::save` afterwards
    /// to ensure modifications are persisted to disk.
    pub fn set_default_server(&mut self, nickname_or_host_or_url: &str) -> anyhow::Result<()> {
        let (host, _) = host_or_url_to_host_and_protocol(nickname_or_host_or_url);
        self.home.set_default_server(host)
    }

    /// Delete a `ServerConfig` from the home configuration.
    ///
    /// Returns an `Err` if `nickname_or_host_or_url`
    /// does not refer to an existing `ServerConfig`
    /// in the home configuration.
    ///
    /// Callers should call `Config::save` afterwards
    /// to ensure modifications are persisted to disk.
    pub fn remove_server(&mut self, nickname_or_host_or_url: &str) -> anyhow::Result<()> {
        let (host, _) = host_or_url_to_host_and_protocol(nickname_or_host_or_url);
        self.home.remove_server(host)
    }

    /// Get a URL for the specified `server`.
    ///
    /// Returns the URL of the default server if `server` is `None`.
    ///
    /// If `server` is `Some` and is a complete URL,
    /// including protocol and hostname,
    /// returns that URL without accessing the configuration.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but not a complete URL,
    ///   and the supplied name does not refer to any server
    ///   in the configuration.
    /// - `server` is `None`, but the configuration does not have a default server.
    pub fn get_host_url(&self, server: Option<&str>) -> anyhow::Result<String> {
        Ok(format!("{}://{}", self.protocol(server)?, self.host(server)?))
    }

    /// Get the hostname of the specified `server`.
    ///
    /// Returns the hostname of the default server if `server` is `None`.
    ///
    /// If `server` is `Some` and is a complete URL,
    /// including protocol and hostname,
    /// returns that hostname without accessing the configuration.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but not a complete URL,
    ///   and the supplied name does not refer to any server
    ///   in the configuration.
    /// - `server` is `None`, but the configuration does not
    ///   have a default server.
    pub fn host<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            if contains_protocol(server) {
                Ok(host_or_url_to_host_and_protocol(server).0)
            } else {
                self.home.host(server)
            }
        } else {
            self.home.default_host()
        }
    }

    /// Get the protocol of the specified `server`, either `"http"` or `"https"`.
    ///
    /// Returns the protocol of the default server if `server` is `None`.
    ///
    /// If `server` is `Some` and is a complete URL,
    /// including protocol and hostname,
    /// returns that protocol without accessing the configuration.
    /// In that case, the protocol is not validated.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but not a complete URL,
    ///   and the supplied name does not refer to any server
    ///   in the configuration.
    /// - `server` is `None`, but the configuration does not have a default server.
    pub fn protocol<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            if contains_protocol(server) {
                Ok(host_or_url_to_host_and_protocol(server).1.unwrap())
            } else {
                self.home.protocol(server)
            }
        } else {
            self.home.default_protocol()
        }
    }

    pub fn default_identity(&self, server: Option<&str>) -> anyhow::Result<&str> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.home.default_identity(host)
        } else {
            self.home.default_server_default_identity()
        }
    }

    /// Set the default identity for `server` in the home configuration.
    ///
    /// Does not validate that `default_identity` applies to `server`.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but does not refer to any server
    ///   in the home configuration.
    /// - `server` is `None`, but the home configuration
    ///   does not have a default server.
    pub fn set_default_identity(&mut self, default_identity: String, server: Option<&str>) -> anyhow::Result<()> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.home.set_server_default_identity(host, default_identity)
        } else {
            self.home.set_default_server_default_identity(default_identity)
        }
    }

    pub fn server_configs(&self) -> &[ServerConfig] {
        &self.home.server_configs
    }

    fn find_config_path(config_dir: &Path) -> Option<PathBuf> {
        [DOT_SPACETIME_FILENAME, SPACETIME_FILENAME, CONFIG_FILENAME]
            .iter()
            .map(|filename| config_dir.join(filename))
            .find(|path| path.exists())
    }

    fn system_config_path() -> PathBuf {
        if let Some(config_path) = std::env::var_os("SPACETIME_CONFIG_FILE") {
            config_path.into()
        } else {
            let mut config_path = dirs::home_dir().unwrap();
            config_path.push(HOME_CONFIG_DIR);
            Self::find_config_path(&config_path).unwrap_or_else(|| config_path.join(CONFIG_FILENAME))
        }
    }

    fn load_from_file(config_path: &Path) -> anyhow::Result<RawConfig> {
        let text = fs::read_to_string(config_path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn load() -> anyhow::Result<Self> {
        let home_path = Self::system_config_path();
        let config = if home_path.exists() {
            Self {
                home: Self::load_from_file(&home_path)
                    .inspect_err(|e| eprintln!("config file {home_path:?} is invalid: {e:#?}"))?,
            }
        } else {
            let config = Self {
                home: RawConfig::new_with_localhost(),
            };
            config.save();
            config
        };
        Ok(config)
    }

    #[doc(hidden)]
    /// Used in tests.
    pub fn new_with_localhost() -> Self {
        Self {
            home: RawConfig::new_with_localhost(),
        }
    }

    pub fn save(&self) {
        let home_path = Self::system_config_path();
        // If the `home_path` is in a directory, ensure it exists.
        create_parent_dir(home_path.as_ref()).unwrap();

        let config = toml::to_string_pretty(&self.home).unwrap();

        eprintln!("Saving config to {}.", home_path.display());
        // TODO: We currently have a race condition if multiple processes are modifying the config.
        // If process X and process Y read the config, each make independent changes, and then save
        // the config, the first writer will have its changes clobbered by the second writer.
        //
        // We used to use `Lockfile` to prevent this from happening, but we had other issues with
        // that approach (see https://github.com/clockworklabs/SpacetimeDB/issues/1339, and the
        // TODO in `lockfile.rs`).
        //
        // We should address this issue, but we currently don't expect it to arise very frequently
        // (see https://github.com/clockworklabs/SpacetimeDB/pull/1341#issuecomment-2150857432).
        if let Err(e) = atomic_write(&home_path, config) {
            eprintln!("Could not save config file: {e}")
        }
    }

    pub fn server_decoding_key(&self, server: Option<&str>) -> anyhow::Result<DecodingKey> {
        self.server_fingerprint(server).and_then(|fing| {
            if let Some(fing) = fing {
                DecodingKey::from_ec_pem(fing.as_bytes()).with_context(|| {
                    format!(
                        "Unable to parse invalid saved server fingerprint as ECDSA public key.
Update the server's fingerprint with:
\tspacetime server fingerprint -s {}",
                        server.unwrap_or("")
                    )
                })
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
            self.home.default_server().map(ServerConfig::nick_or_host)
        }
    }

    pub fn server_fingerprint(&self, server: Option<&str>) -> anyhow::Result<Option<&str>> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.home.server_fingerprint(host)
        } else {
            self.home.default_server_fingerprint()
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

    pub fn edit_server(
        &mut self,
        server: &str,
        new_nickname: Option<&str>,
        new_host: Option<&str>,
        new_protocol: Option<&str>,
    ) -> anyhow::Result<(Option<String>, Option<String>, Option<String>)> {
        let (host, _) = host_or_url_to_host_and_protocol(server);
        self.home.edit_server(host, new_nickname, new_host, new_protocol)
    }

    pub fn delete_server_fingerprint(&mut self, server: Option<&str>) -> anyhow::Result<()> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.home.delete_server_fingerprint(host)
        } else {
            self.home.delete_default_server_fingerprint()
        }
    }

    pub fn set_web_session_id(&mut self, token: String) {
        self.home.set_web_session_id(token);
    }

    pub fn set_spacetimedb_token(&mut self, token: String) {
        self.home.set_spacetimedb_token(token);
    }

    pub fn web_session_id(&self) -> Option<&String> {
        self.home.web_session_id.as_ref()
    }

    pub fn spacetimedb_token(&self) -> Option<&String> {
        self.home.spacetimedb_token.as_ref()
    }

    pub fn spacetimedb_token_or_error(&self) -> anyhow::Result<&String> {
        if let Some(token) = self.spacetimedb_token() {
            Ok(token)
        } else {
            Err(anyhow::anyhow!("No login token found. Please run `spacetime login`."))
        }
    }
}
