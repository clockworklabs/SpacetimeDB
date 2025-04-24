use std::iter::Sum;
use std::ops::Add;

use reqwest::{header, Client, RequestBuilder};
use serde::Deserialize;

use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::Identity;

use crate::util::{AuthHeader, ResponseExt,
map_request_error // fn and macro
};
use crate::util;
use std::path::PathBuf;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Debug, Clone)]
pub struct Connection {
    pub(crate) host: String,
    pub(crate) database_identity: Identity,
    pub(crate) database: String,
    pub(crate) auth_header: AuthHeader,
    // FIXME: bad idea to put these next ones here? else pass'em as arg?
    pub(crate) trust_server_cert_path: Option<PathBuf>,
    pub(crate) client_cert_path: Option<PathBuf>,
    pub(crate) client_key_path: Option<PathBuf>,
    pub(crate) trust_system: bool,
}

impl Connection {
    pub fn db_uri(&self, endpoint: &str) -> String {
        [
            &self.host,
            "/v1/database/",
            &self.database_identity.to_hex(),
            "/",
            endpoint,
        ]
        .concat()
    }
}

pub fn build_client(con: &Connection) -> Client {
    let trust_server_cert_path=con.trust_server_cert_path.as_deref();
    let client_cert_path=con.client_cert_path.as_deref();
    let client_key_path=con.client_key_path.as_deref();
    let trust_system=con.trust_system;
    //XXX: alternatively make this async and then make new() async, and ensure callers do .await on it
    let mut builder = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(util::configure_tls(
                    trust_server_cert_path,
                    client_cert_path,
                    client_key_path,
                    trust_system
                    ))
    })
    .unwrap();
    builder = builder.user_agent(APP_USER_AGENT);

    if let Some(auth_header) = con.auth_header.to_header() {
        let headers = http::HeaderMap::from_iter([(header::AUTHORIZATION, auth_header)]);

        builder = builder.default_headers(headers);
    }

    map_request_error!(
        util::build_client_with_context(builder,
            trust_server_cert_path,
            client_cert_path,
            client_key_path,
            trust_system,
        ), con.host, client_cert_path, client_key_path
    )
    .unwrap()
}

pub struct ClientApi {
    pub con: Connection,
    client: Client,
}

impl ClientApi {
    pub fn new(con: Connection) -> Self {
        let client = build_client(&con);
        Self { con, client }
    }

    pub fn sql(&self) -> RequestBuilder {
        self.client.post(self.con.db_uri("sql"))
    }

    /// Reads the `ModuleDef` from the `schema` endpoint.
    pub async fn module_def(&self) -> anyhow::Result<RawModuleDefV9> {
        let res = self
            .client
            .get(self.con.db_uri("schema"))
            .query(&[("version", "9")])
            .send()
            .await?;
        let DeserializeWrapper(module_def) = res.json_or_error().await?;
        Ok(module_def)
    }

    pub async fn call(&self, reducer_name: &str, arg_json: String) -> anyhow::Result<reqwest::Response> {
        Ok(self
            .client
            .post(self.con.db_uri("call") + "/" + reducer_name)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(arg_json)
            .send()
            .await?)
    }
}

pub(crate) type SqlStmtResult<'a> =
    spacetimedb_client_api_messages::http::SqlStmtResult<&'a serde_json::value::RawValue>;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StmtStats {
    pub total_duration_micros: u64,
    pub rows_inserted: u64,
    pub rows_updated: u64,
    pub rows_deleted: u64,
    pub total_rows: usize,
}

impl Sum<StmtStats> for StmtStats {
    fn sum<I: Iterator<Item = StmtStats>>(iter: I) -> Self {
        iter.fold(StmtStats::default(), Add::add)
    }
}

impl Add for StmtStats {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            total_duration_micros: self.total_duration_micros + rhs.total_duration_micros,
            rows_inserted: self.rows_inserted + rhs.rows_inserted,
            rows_deleted: self.rows_deleted + rhs.rows_deleted,
            rows_updated: self.rows_updated + rhs.rows_updated,
            total_rows: self.total_rows + rhs.total_rows,
        }
    }
}

impl From<&SqlStmtResult<'_>> for StmtStats {
    fn from(value: &SqlStmtResult<'_>) -> Self {
        Self {
            total_duration_micros: value.total_duration_micros,
            rows_inserted: value.stats.rows_inserted,
            rows_deleted: value.stats.rows_deleted,
            rows_updated: value.stats.rows_updated,
            total_rows: value.rows.len(),
        }
    }
}

pub fn from_json_seed<'de, T: serde::de::DeserializeSeed<'de>>(
    s: &'de str,
    seed: T,
) -> Result<T::Value, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_str(s);
    let out = seed.deserialize(&mut de)?;
    de.end()?;
    Ok(out)
}
