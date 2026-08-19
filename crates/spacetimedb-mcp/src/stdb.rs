//! Minimal read-only HTTP client for a running SpacetimeDB instance.
//!
//! Talks to the same `/v1/database/{name_or_identity}/schema` endpoint the
//! `spacetime` CLI uses, and decodes the response into the in-tree
//! `RawModuleDefV9` type. Reusing SpacetimeDB's own schema representation
//! keeps this server in lockstep with the engine instead of reparsing text.

use anyhow::{bail, Context};
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::de::serde::DeserializeWrapper;

/// How to reach SpacetimeDB. The host (and optional auth token) are fixed for
/// the client's lifetime; the target database is supplied per request, so one
/// server can introspect any database on that host.
pub struct Client {
    host: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    /// Build a client for an explicit host and optional bearer token.
    pub fn new(host: impl Into<String>, token: Option<String>) -> Self {
        Self {
            host: host.into(),
            token,
            http: reqwest::Client::new(),
        }
    }

    /// Build a client from the environment.
    ///
    /// `SPACETIMEDB_HOST` defaults to a local instance. `SPACETIMEDB_TOKEN`,
    /// when set, is sent as a bearer token so private databases are reachable.
    pub fn from_env() -> Self {
        let host = std::env::var("SPACETIMEDB_HOST").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let token = std::env::var("SPACETIMEDB_TOKEN").ok().filter(|t| !t.is_empty());
        Self::new(host, token)
    }

    /// Fetch and decode the module definition (schema) for `database`, which
    /// may be either a database name or an identity.
    pub async fn module_def(&self, database: &str) -> anyhow::Result<RawModuleDefV9> {
        let host = self.host.trim_end_matches('/');
        let url = format!("{host}/v1/database/{database}/schema");
        let mut req = self.http.get(&url).query(&[("version", "9")]);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }

        let res = req
            .send()
            .await
            .with_context(|| format!("could not reach SpacetimeDB at {host} (is the host running and reachable?)"))?;

        let status = res.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::NOT_FOUND {
                bail!("database '{database}' not found at {host} (HTTP 404)");
            }
            let body = res.text().await.unwrap_or_default();
            let detail = if body.is_empty() {
                String::new()
            } else {
                format!(": {}", body.trim())
            };
            bail!("schema request for '{database}' failed with HTTP {status}{detail}");
        }

        let DeserializeWrapper(module_def) = res
            .json::<DeserializeWrapper<RawModuleDefV9>>()
            .await
            .context("decoding schema response (unexpected format from the host)")?;
        Ok(module_def)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::introspect;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Bind a throwaway HTTP server that answers one request with `body`, and
    /// return its base URL. Lets us exercise the real fetch + decode path
    /// without a live SpacetimeDB instance.
    async fn serve_once(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut scratch = [0u8; 2048];
            let _ = sock.read(&mut scratch).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(response.as_bytes()).await.unwrap();
            sock.flush().await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn module_def_fetches_and_decodes() {
        use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9Builder;
        use spacetimedb_lib::sats::{AlgebraicType, ProductType};

        // Serialize a schema the way the host would, then serve it back.
        let mut b = RawModuleDefV9Builder::new();
        b.build_table_with_new_type_for_tests("widget", ProductType::from([("id", AlgebraicType::U64)]), false)
            .finish();
        let def = b.finish();
        let body = introspect::schema_json(&def).unwrap();

        let host = serve_once(body).await;
        let fetched = Client::new(host, None).module_def("widget").await.unwrap();
        assert_eq!(introspect::table_names(&fetched), vec!["widget".to_string()]);
    }
}
