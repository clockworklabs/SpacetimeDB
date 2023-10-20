use reqwest::header::IntoHeaderName;
use reqwest::{header, Client, RequestBuilder};
use serde::Deserialize;
use serde_json::value::RawValue;

use spacetimedb_lib::sats::ProductType;
use spacetimedb_lib::Address;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Debug, Clone)]
pub struct Connection {
    pub(crate) host: String,
    pub(crate) address: Address,
    pub(crate) database: String,
    pub(crate) auth_header: Option<String>,
}

pub fn build_headers<'a, K, I>(iter: I) -> header::HeaderMap
where
    K: IntoHeaderName,
    I: IntoIterator<Item = (K, &'a str)>,
{
    let mut headers = header::HeaderMap::new();

    for (k, v) in iter.into_iter() {
        headers.insert(k, header::HeaderValue::from_str(v).unwrap());
    }

    headers
}

pub fn build_client(con: &Connection) -> Client {
    let mut builder = Client::builder().user_agent(APP_USER_AGENT);

    if let Some(auth_header) = &con.auth_header {
        let headers = build_headers([("Authorization", auth_header.as_str())]);

        builder = builder.default_headers(headers);
    }

    builder.build().unwrap()
}

pub struct ClientApi {
    con: Connection,
    client: Client,
}

impl ClientApi {
    pub fn new(con: Connection) -> Self {
        let client = build_client(&con);
        Self { con, client }
    }

    pub fn sql(&self) -> RequestBuilder {
        self.client
            .post(format!("{}/database/sql/{}", self.con.host, self.con.address))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StmtResultJson<'a> {
    pub schema: ProductType,
    #[serde(borrow)]
    pub rows: Vec<&'a RawValue>,
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
