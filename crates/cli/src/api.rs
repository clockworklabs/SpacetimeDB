use reqwest::{header, Client, RequestBuilder};
use serde::Deserialize;
use serde_json::value::RawValue;

use spacetimedb_lib::de::serde::{DeserializeWrapper, SeedWrapper};
use spacetimedb_lib::sats::{ProductType, Typespace};
use spacetimedb_lib::{Address, ModuleDef};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Debug, Clone)]
pub struct Connection {
    pub(crate) host: String,
    pub(crate) address: Address,
    pub(crate) database: String,
    pub(crate) auth_header: Option<String>,
}

impl Connection {
    pub fn db_uri(&self, endpoint: &str) -> String {
        [&self.host, "/database/", endpoint, "/", &self.address.to_hex()].concat()
    }
}

pub fn build_client(con: &Connection) -> Client {
    let mut builder = Client::builder().user_agent(APP_USER_AGENT);

    if let Some(auth_header) = &con.auth_header {
        let headers = http::HeaderMap::from_iter([(header::AUTHORIZATION, auth_header.try_into().unwrap())]);

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
    pub async fn module_def(&self) -> anyhow::Result<ModuleDef> {
        let res = self
            .client
            .get(self.con.db_uri("schema"))
            .query(&[("module_def", true)])
            .send()
            .await?
            .error_for_status()?;
        let DeserializeWrapper(module_def) = res.json().await?;
        Ok(module_def)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StmtResultJson<'a> {
    pub schema: ProductType,
    #[serde(borrow)]
    pub rows: Vec<&'a RawValue>,
}

impl TryFrom<&StmtResultJson<'_>> for spacetimedb::json::client_api::StmtResultJson {
    type Error = serde_json::Error;

    fn try_from(StmtResultJson { schema, rows }: &StmtResultJson<'_>) -> Result<Self, Self::Error> {
        let ty = Typespace::EMPTY.with_type(schema);
        let rows = rows
            .iter()
            .map(|row| from_json_seed(row.get(), SeedWrapper(ty)))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            schema: schema.clone(),
            rows,
        })
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
