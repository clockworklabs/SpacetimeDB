//! Anonymous discovery prints one validated public address, never a constructed
//! hostname, authentication token or private runtime address.

use anyhow::{bail, ensure, Context, Result};
use clap::{Arg, ArgMatches, Command};
use reqwest::{redirect::Policy, Client, StatusCode};
use spacetimedb_lib::{
    container::{endpoints::ContainerEndpoints, MAX_PORTS},
    Identity,
};
use std::{collections::BTreeSet, time::Duration};
use url::Url;

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub(super) fn cli() -> Command {
    Command::new("url")
        .about("Print a container's published HTTPS URL")
        .arg(Arg::new("database").required(true).help("Database name or Identity"))
        .arg(
            Arg::new("port")
                .long("port")
                .help("Declared port name; required when several ports are published"),
        )
        .arg(crate::common_args::server())
        .after_help("Discovery does not start the container or wait for readiness. No login is required.")
}

pub(super) async fn exec(config: &crate::Config, args: &ArgMatches) -> Result<()> {
    let server = args.get_one::<String>("server").map(String::as_str);
    let database = args.get_one::<String>("database").context("database is required")?;
    let port = args.get_one::<String>("port").map(String::as_str);
    let endpoints = fetch(&config.get_host_url(server)?, database).await?;
    println!("{}", select(&endpoints, port)?);
    Ok(())
}

async fn fetch(server: &str, database: &str) -> Result<ContainerEndpoints> {
    ensure!(
        !database.is_empty()
            && !matches!(database, "." | "..")
            && database.len() <= 1024
            && !database.chars().any(char::is_control),
        "invalid database name or Identity"
    );
    let mut url = Url::parse(server).context("invalid server URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "server must be an HTTP or HTTPS origin without credentials or a path"
    );
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid server URL"))?
        .clear()
        .extend(["v1", "database", database, "container", "endpoints"]);
    // This endpoint is public. Do not load a login, resolve an Identity through
    // another request, follow redirects, or forward saved credentials.
    let client = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("could not reach container endpoint discovery on the selected server"))?;
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        bail!("database or container endpoint discovery was not found on the selected server");
    }
    if status == StatusCode::SERVICE_UNAVAILABLE {
        bail!("container endpoints are not available yet; retry shortly");
    }
    if status != StatusCode::OK {
        bail!("container endpoint discovery failed (HTTP {})", status.as_u16());
    }
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_RESPONSE_BYTES as u64),
        "container endpoint discovery response exceeds its size limit"
    );
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("container endpoint discovery response was interrupted"))?
    {
        ensure!(
            chunk.len() <= MAX_RESPONSE_BYTES.saturating_sub(body.len()),
            "container endpoint discovery response exceeds its size limit"
        );
        body.extend_from_slice(&chunk);
    }
    let endpoints: ContainerEndpoints =
        serde_json::from_slice(&body).map_err(|_| anyhow::anyhow!("invalid container endpoint discovery response"))?;
    if let Ok(identity) = database.parse::<Identity>() {
        ensure!(
            identity == endpoints.database_identity,
            "container endpoint discovery returned another database Identity"
        );
    }
    validate(&endpoints)?;
    Ok(endpoints)
}

pub(super) fn validate(endpoints: &ContainerEndpoints) -> Result<()> {
    ensure!(
        endpoints.endpoints.len() <= MAX_PORTS,
        "too many container endpoints in discovery response"
    );
    let mut names = BTreeSet::new();
    for endpoint in &endpoints.endpoints {
        let name = endpoint.name.as_bytes();
        ensure!(
            (1..=32).contains(&name.len())
                && name[0].is_ascii_lowercase()
                && name
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
                && names.insert(&endpoint.name),
            "invalid or duplicate port name in discovery response"
        );
        let address = Url::parse(&endpoint.url).context("invalid container endpoint URL")?;
        ensure!(
            endpoint.url.len() <= 2048
                && endpoint.url.is_ascii()
                && !endpoint.url.bytes().any(|byte| byte.is_ascii_control())
                && address.as_str() == endpoint.url
                && address.scheme() == "https"
                && matches!(address.host(), Some(url::Host::Domain(_)))
                && address.username().is_empty()
                && address.password().is_none()
                && address.port().is_none()
                && address.query().is_none()
                && address.fragment().is_none()
                && address.path() == "/",
            "discovery returned an invalid public HTTPS endpoint"
        );
    }
    Ok(())
}

fn select<'a>(endpoints: &'a ContainerEndpoints, port: Option<&str>) -> Result<&'a str> {
    match (port, endpoints.endpoints.as_slice()) {
        (_, []) => bail!("this database has no declared public HTTP ports"),
        (None, [endpoint]) => Ok(&endpoint.url),
        (Some(port), entries) => entries
            .iter()
            .find(|endpoint| endpoint.name == port)
            .map(|endpoint| endpoint.url.as_str())
            .context("the requested port name is not published by this database"),
        (None, entries) => bail!(
            "several ports are published; select one with --port: {}",
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests;
