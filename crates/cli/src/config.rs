use crate::util::{contains_protocol, host_or_url_to_host_and_protocol};
use anyhow::Context;
use jsonwebtoken::DecodingKey;
use serde::{Deserialize, Serialize};
use spacetimedb::config::{set_opt_value, set_table_opt_value};
use spacetimedb_fs_utils::atomic_write;
use spacetimedb_paths::cli::CliTomlPath;
use std::collections::HashMap;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerConfig {
    pub nickname: Option<String>,
    pub host: String,
    pub protocol: String,
    pub ecdsa_public_key: Option<String>,
}

impl ServerConfig {
    /// Generate a new [Table] representing this [ServerConfig].
    pub fn as_table(&self) -> Table {
        let mut table = Table::new();
        set_table_opt_value(&mut table, "nickname", self.nickname.as_deref());
        table["host"] = self.host.as_str().into();
        table["protocol"] = self.protocol.as_str().into();
        set_table_opt_value(&mut table, "ecdsa_public_key", self.ecdsa_public_key.as_deref());
        table
    }

    /// Update an existing [Table] with the values of a [ServerConfig].
    pub fn update_table(edit: &mut Table, from: &ServerConfig) {
        set_table_opt_value(edit, "nickname", from.nickname.as_deref());
        edit["host"] = from.host.as_str().into();
        edit["protocol"] = from.protocol.as_str().into();
        set_table_opt_value(edit, "ecdsa_public_key", from.ecdsa_public_key.as_deref());
    }

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

    pub fn nick_or_host_or_url_is(&self, name: &str) -> bool {
        self.nickname.as_deref() == Some(name) || self.host == name || {
            let (host, _) = host_or_url_to_host_and_protocol(name);
            self.host == host
        }
    }
}

// Any change here must be coordinated with Config::doc
#[derive(Default, Deserialize, Serialize, Debug, Clone)]
pub struct RawConfig {
    default_server: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    server_configs: Vec<ServerConfig>,
    // TODO: Consider how these tokens should look to be backwards-compatible with the future changes (e.g. we may want to allow users to `login` to switch between multiple accounts - what will we cache and where?)
    // TODO: Move these IDs/tokens out of config so we're no longer storing sensitive tokens in a human-edited file.
    web_session_token: Option<String>,
    spacetimedb_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    home: RawConfig,
    home_path: CliTomlPath,
    // The TOML document that was parsed to create `home`.
    // We need to keep it to preserve comments and formatting when saving the config.
    doc: DocumentMut,
}

const NO_DEFAULT_SERVER_ERROR_MESSAGE: &str = "No default server configuration.
Set an existing server as the default with:
\tspacetime server set-default <server>
Or add a new server which will become the default:
\tspacetime server add {server} <url> --default";

fn no_such_server_error(server: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "No such saved server configuration: {server}
Add a new server configuration with:
\tspacetime server add {server} --url <url>",
    )
}

fn hanging_default_server_context(server: &str) -> String {
    format!("Default server does not refer to a saved server configuration: {server}")
}

impl RawConfig {
    fn new_with_localhost() -> Self {
        let local = ServerConfig {
            host: "127.0.0.1:3000".to_string(),
            protocol: "http".to_string(),
            nickname: Some("local".to_string()),
            ecdsa_public_key: None,
        };
        let testnet = ServerConfig {
            host: "testnet.spacetimedb.com".to_string(),
            protocol: "https".to_string(),
            nickname: Some("testnet".to_string()),
            ecdsa_public_key: None,
        };
        RawConfig {
            default_server: local.nickname.clone(),
            server_configs: vec![local, testnet],
            web_session_token: None,
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

    pub fn set_web_session_token(&mut self, token: String) {
        self.web_session_token = Some(token);
    }

    pub fn set_spacetimedb_token(&mut self, token: String) {
        self.spacetimedb_token = Some(token);
    }

    pub fn clear_login_tokens(&mut self) {
        self.web_session_token = None;
        self.spacetimedb_token = None;
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

    pub fn server_configs(&self) -> &[ServerConfig] {
        &self.home.server_configs
    }

    pub fn load(home_path: CliTomlPath) -> anyhow::Result<Self> {
        let home = spacetimedb::config::parse_preserving_config::<RawConfig>(home_path.as_ref())
            .with_context(|| format!("config file {} is invalid", home_path.display()))?;
        Ok(match home {
            Some((doc, home)) => Self { home, home_path, doc },
            None => {
                let config = Self {
                    home: RawConfig::new_with_localhost(),
                    home_path,
                    doc: Default::default(),
                };
                config.save();
                config
            }
        })
    }

    #[doc(hidden)]
    /// Used in tests.
    pub fn new_with_localhost(home_path: CliTomlPath) -> Self {
        Self {
            home: RawConfig::new_with_localhost(),
            home_path,
            doc: Default::default(),
        }
    }

    /// Returns a preserving copy of [Config].
    fn doc(&self) -> DocumentMut {
        let mut doc = self.doc.clone();

        let mut set_value = |key: &str, value: Option<&str>| {
            set_opt_value(&mut doc, key, value);
        };
        set_value("default_server", self.home.default_server.as_deref());
        set_value("web_session_token", self.home.web_session_token.as_deref());
        set_value("spacetimedb_token", self.home.spacetimedb_token.as_deref());

        // Short-circuit if there are no servers.
        if self.home.server_configs.is_empty() {
            doc.remove("server_configs");
            return doc;
        }
        // ... or if there are no server_configs to edit.
        let server_configs = if let Some(cfg) = doc.get_mut("server_configs").and_then(Item::as_array_of_tables_mut) {
            cfg
        } else {
            doc["server_configs"] =
                Item::ArrayOfTables(self.home.server_configs.iter().map(ServerConfig::as_table).collect());
            return doc;
        };

        let mut new_configs = self
            .home
            .server_configs
            .iter()
            .map(|cfg| (cfg.nick_or_host(), cfg))
            .collect::<HashMap<_, _>>();

        // Update the existing servers.
        let mut new_vec = Vec::with_capacity(self.home.server_configs.len());
        for old_config in server_configs.iter_mut() {
            let nick_or_host = old_config
                .get("nickname")
                .or_else(|| old_config.get("host"))
                .and_then(|v| v.as_str())
                .unwrap();
            if let Some(new_config) = new_configs.remove(nick_or_host) {
                ServerConfig::update_table(old_config, new_config);
                new_vec.push(old_config.clone());
            }
        }

        // Add the new servers.
        new_vec.extend(new_configs.values().cloned().map(ServerConfig::as_table));
        *server_configs = ArrayOfTables::from_iter(new_vec);

        doc
    }

    pub fn save(&self) {
        let home_path = &self.home_path;
        // If the `home_path` is in a directory, ensure it exists.
        home_path.create_parent().unwrap();

        let config = self.doc().to_string();

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
        if let Err(e) = atomic_write(&home_path.0, config) {
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
\tspacetime server fingerprint {}",
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

    pub fn set_web_session_token(&mut self, token: String) {
        self.home.set_web_session_token(token);
    }

    pub fn set_spacetimedb_token(&mut self, token: String) {
        self.home.set_spacetimedb_token(token);
    }

    pub fn clear_login_tokens(&mut self) {
        self.home.clear_login_tokens();
    }

    pub fn web_session_token(&self) -> Option<&String> {
        self.home.web_session_token.as_ref()
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

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_paths::cli::CliTomlPath;
    use spacetimedb_paths::FromPathUnchecked;
    use std::fs;

    const CONFIG_FULL: &str = r#"default_server = "local"
web_session_token = "web_session"
spacetimedb_token = "26ac38857c2bd6c5b60ec557ecd4f9add918fef577dc92c01ca96ff08af5b84d"

# comment on table
[[server_configs]]
nickname = "local"
host = "127.0.0.1:3000"
protocol = "http"

[[server_configs]]
nickname = "testnet" # Comment nickname
host = "testnet.spacetimedb.com"
protocol = "https"

# Comment end
"#;
    const CONFIG_FULL_NO_COMMENT: &str = r#"default_server = "local"
web_session_token = "web_session"
spacetimedb_token = "26ac38857c2bd6c5b60ec557ecd4f9add918fef577dc92c01ca96ff08af5b84d"

[[server_configs]]
nickname = "local"
host = "127.0.0.1:3000"
protocol = "http"

[[server_configs]]
nickname = "testnet"
host = "testnet.spacetimedb.com"
protocol = "https"

# Comment end
"#;
    const CONFIG_CHANGE_SERVER: &str = r#"default_server = "local"
web_session_token = "web_session"
spacetimedb_token = "26ac38857c2bd6c5b60ec557ecd4f9add918fef577dc92c01ca96ff08af5b84d"

[[server_configs]]
nickname = "testnet" # Comment nickname
host = "prod.spacetimedb.com"
protocol = "https"

# Comment end
"#;
    const CONFIG_EMPTY: &str = r#"
# Comment end
"#;
    fn check_config<F>(input: &str, output: &str, f: F) -> ResultTest<()>
    where
        F: FnOnce(&mut Config) -> ResultTest<()>,
    {
        let tmp = tempfile::tempdir()?;
        let config_path = CliTomlPath::from_path_unchecked(tmp.path().join("config.toml"));

        fs::write(&config_path, input)?;

        let mut config = Config::load(config_path.clone()).unwrap();
        f(&mut config)?;
        config.save();

        let contents = fs::read_to_string(&config_path)?;

        assert_eq!(contents, output);

        Ok(())
    }

    // Test editing the config file.
    #[test]
    fn test_config_edits() -> ResultTest<()> {
        check_config(CONFIG_FULL, CONFIG_EMPTY, |config| {
            config.home.default_server = None;
            config.home.server_configs.clear();
            config.home.spacetimedb_token = None;
            config.home.web_session_token = None;

            Ok(())
        })?;

        check_config(CONFIG_FULL, CONFIG_CHANGE_SERVER, |config| {
            config.home.server_configs.remove(0);
            config.home.server_configs[0].host = "prod.spacetimedb.com".to_string();
            Ok(())
        })?;

        Ok(())
    }

    // Test adding to the config file.
    #[test]
    fn test_config_adds() -> ResultTest<()> {
        check_config(CONFIG_FULL, CONFIG_FULL, |_| Ok(()))?;
        check_config(CONFIG_EMPTY, CONFIG_EMPTY, |_| Ok(()))?;

        check_config(CONFIG_EMPTY, CONFIG_FULL_NO_COMMENT, |config| {
            config.home.default_server = Some("local".to_string());
            config.home.server_configs = vec![
                ServerConfig {
                    nickname: Some("local".to_string()),
                    host: "127.0.0.1:3000".to_string(),
                    protocol: "http".to_string(),
                    ecdsa_public_key: None,
                },
                ServerConfig {
                    nickname: Some("testnet".to_string()),
                    host: "testnet.spacetimedb.com".to_string(),
                    protocol: "https".to_string(),
                    ecdsa_public_key: None,
                },
            ];
            config.home.spacetimedb_token =
                Some("26ac38857c2bd6c5b60ec557ecd4f9add918fef577dc92c01ca96ff08af5b84d".to_string());
            config.home.web_session_token = Some("web_session".to_string());

            Ok(())
        })?;

        Ok(())
    }
}
