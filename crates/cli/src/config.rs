use crate::util::{contains_protocol, host_or_url_to_host_and_protocol};
use anyhow::Context;
use jsonwebtoken::DecodingKey;
use serde::{Deserialize, Serialize};
use spacetimedb::auth::identity::decode_token;
use spacetimedb_lib::Identity;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdentityConfig {
    pub nickname: Option<String>,
    pub identity: Identity,
    pub token: String,
}

impl IdentityConfig {
    pub fn nick_or_identity(&self) -> impl std::fmt::Display + '_ {
        if let Some(nick) = &self.nickname {
            itertools::Either::Left(nick)
        } else {
            itertools::Either::Right(&self.identity)
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

    fn assert_identity_applies(&self, id: &IdentityConfig) -> anyhow::Result<()> {
        if let Some(fingerprint) = &self.ecdsa_public_key {
            let decoder = DecodingKey::from_ec_pem(fingerprint.as_bytes()).with_context(|| {
                let server = self.nick_or_host();
                format!(
                    "Cannot verify tokens using invalid saved fingerprint from server: {server}
Update the fingerprint with:
\tspacetime server fingerprint {server}",
                )
            })?;
            decode_token(&decoder, &id.token).map_err(|_| {
                let id_name = id.nick_or_identity();
                let server_name = self.nick_or_host();
                anyhow::anyhow!(
                    "Identity {id_name} is not valid for server {server_name}
List valid identities for server {server_name} with:
\tspacetime identity list -s {server_name}",
                )
            })?;
        }
        Ok(())
    }
}

#[derive(Default, Deserialize, Serialize, Debug, Clone)]
pub struct RawConfig {
    default_server: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    identity_configs: Vec<IdentityConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    server_configs: Vec<ServerConfig>,
}

const DEFAULT_HOST: &str = "127.0.0.1:3000";
const DEFAULT_PROTOCOL: &str = "http";
const DEFAULT_HOST_NICKNAME: &str = "local";

#[derive(Clone)]
pub struct Config {
    proj: RawConfig,
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
        RawConfig {
            default_server: Some(DEFAULT_HOST_NICKNAME.to_string()),
            identity_configs: Vec::new(),
            server_configs: vec![
                ServerConfig {
                    default_identity: None,
                    host: DEFAULT_HOST.to_string(),
                    protocol: DEFAULT_PROTOCOL.to_string(),
                    nickname: Some(DEFAULT_HOST_NICKNAME.to_string()),
                    ecdsa_public_key: None,
                },
                ServerConfig {
                    default_identity: None,
                    host: "testnet.spacetimedb.com".to_string(),
                    protocol: "https".to_string(),
                    nickname: Some("testnet".to_string()),
                    ecdsa_public_key: None,
                },
            ],
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

    fn find_identity_config(&self, identity: &str) -> anyhow::Result<&IdentityConfig> {
        for cfg in &self.identity_configs {
            if cfg.nickname.as_deref() == Some(identity) || &*cfg.identity.to_hex() == identity {
                return Ok(cfg);
            }
        }
        Err(anyhow::anyhow!(
            "No such saved identity configuration: {identity}
Import an existing identity with:
\tspacetime identity import <identity> <token>",
        ))
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

    fn assert_identity_matches_server(&self, server: &str, identity: &str) -> anyhow::Result<()> {
        let ident = self
            .find_identity_config(identity)
            .with_context(|| format!("Cannot verify that unknown identity {identity} applies to server {server}",))?;
        let server_cfg = self
            .find_server(server)
            .with_context(|| format!("Cannot verify that identity {identity} applies to unknown server {server}",))?;
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
            self.assert_identity_matches_server(default_server, &default_identity)
                .with_context(|| {
                    format!("Cannot set {default_identity} as default identity for server {default_server}")
                })?;

            // Unfortunate clone,
            // because `set_server_default_identity` needs a unique ref to `self`.
            let def = default_server.to_string();
            self.set_server_default_identity(&def, default_identity)
        } else {
            Err(anyhow::anyhow!(NO_DEFAULT_SERVER_ERROR_MESSAGE))
        }
    }

    fn unset_all_default_identities(&mut self) {
        for cfg in &mut self.server_configs {
            cfg.default_identity = None;
        }
    }

    fn update_all_default_identities(&mut self) {
        for server in &mut self.server_configs {
            if let Some(default_identity) = &server.default_identity {
                // can't use find_identity_config because of borrow checker
                if self.identity_configs.iter().any(|cfg| {
                    cfg.nickname.as_deref() == Some(&**default_identity) || &*cfg.identity.to_hex() == default_identity
                }) {
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
        // Check that such a server exists before setting the default.
        self.find_server(server)
            .with_context(|| format!("Cannot set default server to unknown server {server}"))?;

        self.default_server = Some(server.to_string());

        Ok(())
    }

    /// Implements `spacetime server remove`.
    fn remove_server(&mut self, server: &str, delete_identities: bool) -> anyhow::Result<Vec<IdentityConfig>> {
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

            // If requested, delete all identities which match the server.
            // This requires a fingerprint.
            let deleted_ids = if delete_identities {
                let fingerprint = cfg.ecdsa_public_key.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Cannot delete identities for server without saved identity: {server}
Fetch the server's fingerprint with:
\tspacetime server fingerprint {server}"
                    )
                })?;
                self.remove_identities_for_fingerprint(&fingerprint)?
            } else {
                Vec::new()
            };

            return Ok(deleted_ids);
        }
        Err(no_such_server_error(server))
    }

    fn remove_identities_for_fingerprint(&mut self, fingerprint: &str) -> anyhow::Result<Vec<IdentityConfig>> {
        let decoder = DecodingKey::from_ec_pem(fingerprint.as_bytes()).with_context(|| {
            "Cannot delete identities for server without saved identity: {server}
Fetch the server's fingerprint with:
\tspacetime server fingerprint {server}"
        })?;

        // TODO: use `Vec::extract_if` instead when it stabilizes.
        let (to_keep, to_discard) = self
            .identity_configs
            .drain(..)
            .partition(|cfg| decode_token(&decoder, &cfg.token).is_err());
        self.identity_configs = to_keep;
        Ok(to_discard)
    }

    /// Remove all stored `IdentityConfig`s which apply to the server named by `server`.
    ///
    /// Implements `spacetime identity remove --all-server`.
    fn remove_identities_for_server(&mut self, server: &str) -> anyhow::Result<Vec<IdentityConfig>> {
        // Have to find the server config manually instead of doing `find_server_mut`
        // because we need to mutably borrow multiple components of `self`.
        if let Some(cfg) = self
            .server_configs
            .iter_mut()
            .find(|cfg| cfg.nick_or_host_or_url_is(server))
        {
            let fingerprint = cfg
                .ecdsa_public_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No fingerprint saved for server: {}", server))?;
            return self.remove_identities_for_fingerprint(&fingerprint);
        }
        Err(no_such_server_error(server))
    }

    /// Remove all storied `IdentityConfig`s which apply to the default server.
    fn remove_identities_for_default_server(&mut self) -> anyhow::Result<Vec<IdentityConfig>> {
        if let Some(default_server) = &self.default_server {
            let default_server = default_server.clone();
            self.remove_identities_for_server(&default_server)
        } else {
            Err(anyhow::anyhow!(NO_DEFAULT_SERVER_ERROR_MESSAGE))
        }
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
\tspacetime server fingerprint {server}"
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
}

impl Config {
    pub fn default_server_name(&self) -> Option<&str> {
        self.proj
            .default_server
            .as_deref()
            .or(self.home.default_server.as_deref())
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
    /// If `delete_identities` is true,
    /// also removes any saved `IdentityConfig`s
    /// which apply to the removed server.
    /// This requires that the server have a saved fingerprint.
    ///
    /// Callers should call `Config::save` afterwards
    /// to ensure modifications are persisted to disk.
    pub fn remove_server(
        &mut self,
        nickname_or_host_or_url: &str,
        delete_identities: bool,
    ) -> anyhow::Result<Vec<IdentityConfig>> {
        let (host, _) = host_or_url_to_host_and_protocol(nickname_or_host_or_url);
        self.home.remove_server(host, delete_identities)
    }

    /// Get a URL for the specified `server`.
    ///
    /// Returns the URL of the default server if `server` is `None`.
    ///
    /// Entries in the project configuration supersede entries in the home configuration.
    ///
    /// If `server` is `Some` and is a complete URL,
    /// including protocol and hostname,
    /// returns that URL without accessing the configuration.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but not a complete URL,
    ///   and the supplied name does not refer to any server
    ///   in either the project or the home configuration.
    /// - `server` is `None`, but neither the home nor the project configuration
    ///   has a default server.
    pub fn get_host_url(&self, server: Option<&str>) -> anyhow::Result<String> {
        Ok(format!("{}://{}", self.protocol(server)?, self.host(server)?))
    }

    /// Get the hostname of the specified `server`.
    ///
    /// Returns the hostname of the default server if `server` is `None`.
    ///
    /// Entries in the project configuration supersede entries in the home configuration.
    ///
    /// If `server` is `Some` and is a complete URL,
    /// including protocol and hostname,
    /// returns that hostname without accessing the configuration.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but not a complete URL,
    ///   and the supplied name does not refer to any server
    ///   in either the project or the home configuration.
    /// - `server` is `None`, but neither the home nor the project configuration
    ///   has a default server.
    pub fn host<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            if contains_protocol(server) {
                Ok(host_or_url_to_host_and_protocol(server).0)
            } else {
                self.proj.host(server).or_else(|_| self.home.host(server))
            }
        } else {
            self.proj.default_host().or_else(|_| self.home.default_host())
        }
    }

    /// Get the protocol of the specified `server`, either `"http"` or `"https"`.
    ///
    /// Returns the protocol of the default server if `server` is `None`.
    ///
    /// Entries in the project configuration supersede entries in the home configuration.
    ///
    /// If `server` is `Some` and is a complete URL,
    /// including protocol and hostname,
    /// returns that protocol without accessing the configuration.
    /// In that case, the protocol is not validated.
    ///
    /// Returns an `Err` if:
    /// - `server` is `Some`, but not a complete URL,
    ///   and the supplied name does not refer to any server
    ///   in either the project or the home configuration.
    /// - `server` is `None`, but neither the home nor the project configuration
    ///   has a default server.
    pub fn protocol<'a>(&'a self, server: Option<&'a str>) -> anyhow::Result<&'a str> {
        if let Some(server) = server {
            if contains_protocol(server) {
                Ok(host_or_url_to_host_and_protocol(server).1.unwrap())
            } else {
                self.proj.protocol(server).or_else(|_| self.home.protocol(server))
            }
        } else {
            self.proj.default_protocol().or_else(|_| self.home.default_protocol())
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

    /// Sets the `nickname` for the provided `identity`.
    ///
    /// If the `identity` already has a `nickname` set, it will be overwritten and returned. If the
    /// `identity` is not found, an error will be returned.
    ///
    /// # Returns
    /// * `Ok(Option<String>)` - If the identity was found, the old nickname will be returned.
    /// * `Err(anyhow::Error)` - If the identity was not found.
    pub fn set_identity_nickname(
        &mut self,
        identity: &Identity,
        nickname: &str,
    ) -> Result<Option<String>, anyhow::Error> {
        let config = self
            .home
            .identity_configs
            .iter_mut()
            .find(|c| c.identity == *identity)
            .ok_or_else(|| anyhow::anyhow!("Identity {} not found", identity))?;
        let old_nickname = std::mem::replace(&mut config.nickname, Some(nickname.to_string()));
        Ok(old_nickname)
    }

    pub fn identity_configs(&self) -> &[IdentityConfig] {
        &self.home.identity_configs
    }

    pub fn identity_configs_mut(&mut self) -> &mut Vec<IdentityConfig> {
        &mut self.home.identity_configs
    }

    pub fn server_configs(&self) -> &[ServerConfig] {
        &self.home.server_configs
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
            return if is_project {
                // Return an empty config without creating a file.
                RawConfig::default()
            } else {
                // Return a default config with http://127.0.0.1:3000 as the default server.
                // Do not (yet) create a file.
                // The config file will be created later by `Config::save` if necessary.
                RawConfig::new_with_localhost()
            };
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
        let home_config = Self::load_raw(home_dir.join(HOME_CONFIG_DIR), false);

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

    pub fn new_with_localhost() -> Self {
        Self {
            home: RawConfig::new_with_localhost(),
            proj: RawConfig::default(),
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
        let default_identity = self.default_identity(server)?;
        self.get_identity_config(default_identity).ok_or_else(|| {
            anyhow::anyhow!(
                "No saved configuration for identity: {default_identity}
Import an existing identity with:
\tspacetime identity import <identity> <token>"
            )
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

    pub fn get_identity_config_by_identity(&self, identity: &Identity) -> Option<&IdentityConfig> {
        self.identity_configs().iter().find(|c| c.identity == *identity)
    }

    pub fn get_identity_config_by_identity_mut(&mut self, identity: &Identity) -> Option<&mut IdentityConfig> {
        self.identity_configs_mut().iter_mut().find(|c| c.identity == *identity)
    }

    /// Converts some given `identity_or_name` into an identity.
    ///
    /// If `identity_or_name` is `None` then `None` is returned. If `identity_or_name` is `Some`,
    /// then if its an identity then its just returned. If its not an identity it is assumed to be
    /// a name and it is looked up as an identity nickname. If the identity exists it is returned,
    /// otherwise we panic.
    pub fn resolve_name_to_identity(&self, identity_or_name: &str) -> anyhow::Result<Identity> {
        let cfg = self
            .get_identity_config(identity_or_name)
            .ok_or_else(|| anyhow::anyhow!("No such identity: {}", identity_or_name))?;
        Ok(cfg.identity)
    }

    /// Converts some given `identity_or_name` into an `IdentityConfig`.
    ///
    /// # Returns
    /// * `None` - If an identity config with the given `identity_or_name` does not exist.
    /// * `Some` - A mutable reference to the `IdentityConfig` with the given `identity_or_name`.
    pub fn get_identity_config(&self, identity_or_name: &str) -> Option<&IdentityConfig> {
        if let Ok(identity) = Identity::from_hex(identity_or_name) {
            self.get_identity_config_by_identity(&identity)
        } else {
            self.identity_configs()
                .iter()
                .find(|c| c.nickname.as_deref() == Some(identity_or_name))
        }
    }

    /// Converts some given `identity_or_name` into a mutable `IdentityConfig`.
    ///
    /// # Returns
    /// * `None` - If an identity config with the given `identity_or_name` does not exist.
    /// * `Some` - A mutable reference to the `IdentityConfig` with the given `identity_or_name`.
    pub fn get_identity_config_mut(&mut self, identity_or_name: &str) -> Option<&mut IdentityConfig> {
        if let Ok(identity) = Identity::from_hex(identity_or_name) {
            self.get_identity_config_by_identity_mut(&identity)
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
            .iter()
            .position(|c| c.nickname.as_deref() == Some(name));
        if let Some(index) = index {
            Some(self.home.identity_configs.remove(index))
        } else {
            None
        }
    }

    pub fn delete_identity_config_by_identity(&mut self, identity: &Identity) -> Option<IdentityConfig> {
        let index = self.home.identity_configs.iter().position(|c| c.identity == *identity);
        if let Some(index) = index {
            Some(self.home.identity_configs.remove(index))
        } else {
            None
        }
    }

    /// Deletes all stored identity configs. This function does not save the config after removing
    /// all configs.
    pub fn delete_all_identity_configs(&mut self) {
        self.home.identity_configs.clear();
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

    pub fn remove_identities_for_server(&mut self, server: Option<&str>) -> anyhow::Result<Vec<IdentityConfig>> {
        if let Some(server) = server {
            let (host, _) = host_or_url_to_host_and_protocol(server);
            self.home.remove_identities_for_server(host)
        } else {
            self.home.remove_identities_for_default_server()
        }
    }

    pub fn remove_identities_for_fingerprint(&mut self, fingerprint: &str) -> anyhow::Result<Vec<IdentityConfig>> {
        self.home.remove_identities_for_fingerprint(fingerprint)
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
}
