//! Minimal read-only HTTP client for a running SpacetimeDB instance.
//!
//! Talks to the same `/v1/database/{name_or_identity}/schema` endpoint the
//! `spacetime` CLI uses, and decodes the response into the in-tree
//! `RawModuleDefV9` type. Reusing SpacetimeDB's own schema representation
//! keeps this server in lockstep with the engine instead of reparsing text.

use anyhow::Context;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::de::serde::DeserializeWrapper;

/// How to reach SpacetimeDB. The host (and optional auth token) come from the
/// environment; the target database is supplied per request, so one server can
/// introspect any database on that host.
pub struct Client {
    host: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    /// Build a client from the environment.
    ///
    /// `SPACETIMEDB_HOST` defaults to a local instance. `SPACETIMEDB_TOKEN`,
    /// when set, is sent as a bearer token so private databases are reachable.
    pub fn from_env() -> Self {
        let host = std::env::var("SPACETIMEDB_HOST").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let token = std::env::var("SPACETIMEDB_TOKEN").ok().filter(|t| !t.is_empty());
        Self {
            host,
            token,
            http: reqwest::Client::new(),
        }
    }

    /// Fetch and decode the module definition (schema) for `database`, which
    /// may be either a database name or an identity.
    pub async fn module_def(&self, database: &str) -> anyhow::Result<RawModuleDefV9> {
        let url = format!("{}/v1/database/{}/schema", self.host.trim_end_matches('/'), database);
        let mut req = self.http.get(&url).query(&[("version", "9")]);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req
            .send()
            .await
            .with_context(|| format!("requesting schema from {url}"))?
            .error_for_status()
            .with_context(|| format!("schema request to {url} failed"))?;
        let DeserializeWrapper(module_def) = res
            .json::<DeserializeWrapper<RawModuleDefV9>>()
            .await
            .context("decoding schema response")?;
        Ok(module_def)
    }
}
