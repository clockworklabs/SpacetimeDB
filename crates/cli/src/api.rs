use std::iter::Sum;
use std::ops::Add;

use reqwest::{header, Client, RequestBuilder};
use serde::Deserialize;
use serde_json::value::RawValue;

use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::sats::ProductType;
use spacetimedb_lib::Identity;

use crate::util::{AuthHeader, ResponseExt};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Debug, Clone)]
pub struct Connection {
    pub(crate) host: String,
    pub(crate) database_identity: Identity,
    pub(crate) database: String,
    pub(crate) auth_header: AuthHeader,
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
    let mut builder = Client::builder().user_agent(APP_USER_AGENT);

    if let Some(auth_header) = con.auth_header.to_header() {
        let headers = http::HeaderMap::from_iter([(header::AUTHORIZATION, auth_header)]);

        builder = builder.default_headers(headers);
    }

    builder.build().unwrap()
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

// Sync with spacetimedb::json::client_api::StmtResultJson
#[derive(Debug, Clone, Deserialize)]
pub struct StmtResultJson<'a> {
    pub schema: ProductType,
    #[serde(borrow)]
    pub rows: Vec<&'a RawValue>,
    pub total_duration_micros: u64,
    #[serde(default)]
    pub stats: StmtStatsJson,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StmtStatsJson {
    pub rows_inserted: u64,
    pub rows_deleted: u64,
    pub rows_updated: u64,
}

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

impl From<&StmtResultJson<'_>> for StmtStats {
    fn from(value: &StmtResultJson<'_>) -> Self {
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
