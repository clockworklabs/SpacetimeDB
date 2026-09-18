//! Explicit platform discovery and bounded, target-specific credential requests.

use crate::Identity;
use http::Uri;
use reqwest::{header, redirect::Policy, Client, Response, Url};
use serde::Deserialize;
use std::{
    fmt,
    net::IpAddr,
    path::{Component, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;

const DATABASE_IDENTITY: &str = "SPACETIMEDB_DATABASE_IDENTITY";
const SERVER_URI: &str = "SPACETIMEDB_SERVER_URI";
const CREDENTIAL_BROKER: &str = "SPACETIMEDB_CREDENTIAL_BROKER";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_LIFETIME: Duration = Duration::from_secs(30);
const MAX_TOKEN: usize = 8192;
const MAX_BODY: usize = MAX_TOKEN + 256;
const MAX_HEADERS: usize = 4096;
const MAX_URI: usize = 4096;
// The Unix transport uses the same HTTP authority and request path as the
// guest proxy. This URL is never resolved or connected over TCP in Unix mode.
const LOCAL_HTTP_URI: &str = "http://127.0.0.1:18081/v1/credentials";

/// A credential error whose diagnostics never include discovery values,
/// response bodies, request headers, or tokens.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerCredentialError {
    #[error("Missing or non-Unicode container discovery variable {0}")]
    MissingEnvironment(&'static str),
    #[error("Invalid container discovery configuration")]
    InvalidDiscovery,
    #[error("Unsupported container credential transport")]
    UnsupportedTransport,
    #[error("Container credentials cannot be combined with a static token")]
    ConflictingCredentials,
    #[error("Container credentials cannot be used with debug files that record tokens")]
    DebugLogging,
    #[error("Invalid container credential target")]
    InvalidTarget,
    #[error("Unable to resolve the database Identity on the selected server")]
    NameResolution,
    #[error("Container credential request was denied")]
    Denied,
    #[error("Container credential broker is unavailable")]
    Unavailable,
    #[error("Container credential request failed")]
    Transport,
    #[error("Container credential request exceeded its deadline")]
    Timeout,
    #[error("Invalid container credential response")]
    InvalidResponse,
    #[error("Container credential expired before the connection completed")]
    Expired,
}

type Result<T> = std::result::Result<T, ContainerCredentialError>;

#[derive(Clone)]
enum Endpoint {
    Http(Url),
    Unix(PathBuf),
}

/// Discovery configuration for a process running in a SpacetimeDB container.
///
/// This value contains no credentials. Cloning it never copies a token. The
/// broker derives the sender from its authenticated runtime association, not
/// from the Identity in this configuration or from request fields.
///
/// ```ignore
/// use spacetimedb_sdk::credentials;
/// let container = credentials::Container::from_env()?;
/// let connection = DbConnection::builder()
///     .with_container_credentials(container)
///     .build_async().await?;
/// ```
///
/// The builder defaults to this container's database and server. Use its
/// `with_uri` and `with_database_name` methods to select another database.
/// Explicit `self` resolves locally to [`Self::database_identity`].
///
/// Each builder connection requests a fresh token. To renew credentials and
/// reconnect with fresh subscriptions, use [`crate::ContainerSession`]. An
/// ordinary builder connection ends when its credential expires or is revoked.
/// Do not save the token returned by `on_connect` for later use.
#[derive(Clone)]
pub struct Container {
    identity: Identity,
    server_uri: Uri,
    endpoint: Endpoint,
}

impl fmt::Debug for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Container").finish_non_exhaustive()
    }
}

impl Container {
    /// Read only the three platform-injected discovery variables. Missing or
    /// invalid configuration is an error; this never reads stored CLI/SDK
    /// credentials, allocates an anonymous Identity, or chooses a default server.
    /// No network or socket operation occurs until a token is requested.
    pub fn from_env() -> Result<Self> {
        let read = |key| std::env::var(key).map_err(|_| ContainerCredentialError::MissingEnvironment(key));
        Self::new(&read(DATABASE_IDENTITY)?, &read(SERVER_URI)?, &read(CREDENTIAL_BROKER)?)
    }

    /// Use explicit discovery values instead of reading the environment.
    ///
    /// The broker must be an absolute `unix:///path` socket URI or an HTTP URI
    /// with a numeric loopback address and the path `/v1/credentials`. Unix
    /// sockets are supported on Unix targets. Broker redirects, proxies, DNS
    /// lookup, embedded credentials, and fallback transports are forbidden.
    pub fn new(database_identity: &str, server_uri: &str, broker_uri: &str) -> Result<Self> {
        let identity = parse_identity(database_identity).map_err(|_| ContainerCredentialError::InvalidDiscovery)?;
        let server_uri = server_url(server_uri)?
            .as_str()
            .parse()
            .map_err(|_| ContainerCredentialError::InvalidDiscovery)?;
        Ok(Self {
            identity,
            server_uri,
            endpoint: endpoint(broker_uri)?,
        })
    }

    pub fn database_identity(&self) -> Identity {
        self.identity
    }

    pub fn server_uri(&self) -> &Uri {
        &self.server_uri
    }

    /// Obtain a fresh credential for HTTP or another client of `target`.
    ///
    /// Send it only to that database's trusted server. It is a bearer credential
    /// with a lifetime of at most 30 seconds, possibly less. Keep it in memory;
    /// never put it in environment variables, logs, images, or durable files.
    /// This method neither retries a denial nor falls back to an owner token.
    pub async fn token_for(&self, target: Identity) -> Result<ContainerToken> {
        tokio::time::timeout(REQUEST_TIMEOUT, self.request_token(target))
            .await
            .map_err(|_| ContainerCredentialError::Timeout)?
    }

    async fn request_token(&self, target: Identity) -> Result<ContainerToken> {
        let builder = client_builder();
        let (builder, uri) = match &self.endpoint {
            Endpoint::Http(uri) => (builder, uri.clone()),
            Endpoint::Unix(path) => {
                #[cfg(unix)]
                {
                    (builder.unix_socket(path.clone()), Url::parse(LOCAL_HTTP_URI).unwrap())
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    return Err(ContainerCredentialError::UnsupportedTransport);
                }
            }
        };
        let client = builder.build().map_err(|_| ContainerCredentialError::Transport)?;
        // Identity::to_hex is fixed lowercase ASCII, so the body cannot contain
        // request framing, another sender, or arbitrary JSON fields.
        let body = format!("{{\"target_database\":\"{}\"}}", target.to_hex());
        let response = client
            .post(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONNECTION, "close")
            .body(body)
            .send()
            .await
            .map_err(transport_error)?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => return Err(ContainerCredentialError::Denied),
            503 => return Err(ContainerCredentialError::Unavailable),
            _ => return Err(ContainerCredentialError::InvalidResponse),
        }
        let headers = response.headers();
        if header_size(headers) > MAX_HEADERS
            || headers.get_all(header::CONTENT_TYPE).iter().count() != 1
            || headers.get(header::CONTENT_TYPE).map(|value| value.as_bytes()) != Some(b"application/json")
            || headers.get_all(header::CACHE_CONTROL).iter().count() != 1
            || headers.get(header::CACHE_CONTROL).map(|value| value.as_bytes()) != Some(b"no-store")
            || headers.contains_key(header::TRANSFER_ENCODING)
            || headers.contains_key(header::CONTENT_ENCODING)
            || headers.get_all(header::CONTENT_LENGTH).iter().count() != 1
        {
            return Err(ContainerCredentialError::InvalidResponse);
        }
        let length = headers[header::CONTENT_LENGTH]
            .to_str()
            .ok()
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|length| *length > 0 && *length <= MAX_BODY)
            .ok_or(ContainerCredentialError::InvalidResponse)?;
        let bytes = bounded_body(response, length).await?;
        if bytes.len() != length {
            return Err(ContainerCredentialError::InvalidResponse);
        }
        let response: TokenResponse =
            serde_json::from_slice(&bytes).map_err(|_| ContainerCredentialError::InvalidResponse)?;
        let expiry = UNIX_EPOCH
            .checked_add(Duration::from_secs(response.expires_unix_seconds))
            .ok_or(ContainerCredentialError::InvalidResponse)?;
        let remaining = expiry
            .duration_since(SystemTime::now())
            .ok()
            .filter(|remaining| !remaining.is_zero() && *remaining <= MAX_LIFETIME)
            .ok_or(ContainerCredentialError::InvalidResponse)?;
        if response.token.is_empty()
            || response.token.len() > MAX_TOKEN
            || !response.token.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(ContainerCredentialError::InvalidResponse);
        }
        // Retain a monotonic cap even if the system clock later moves backward.
        let deadline = Instant::now() + remaining;
        Ok(ContainerToken {
            token: response.token,
            target,
            expiry,
            deadline,
        })
    }

    pub(crate) async fn prepare_connection(&self, uri: Option<&Uri>, target: Option<&str>) -> Result<Prepared> {
        let (uri, target) = self.resolve_connection_target(uri, target).await?;
        let credential = self.token_for(target).await?;
        Ok(Prepared {
            uri,
            target: target.to_hex().to_string(),
            credential,
        })
    }

    pub(crate) async fn resolve_connection_target(
        &self,
        uri: Option<&Uri>,
        target: Option<&str>,
    ) -> Result<(Uri, Identity)> {
        let uri = uri.unwrap_or(&self.server_uri);
        let mut server = server_url(&uri.to_string())?;
        let target = match target {
            None | Some("self") => self.identity,
            Some(target) => match parse_identity(target) {
                Ok(identity) => identity,
                Err(_) => {
                    // Preserve the SDK's server path prefix. Append the name as
                    // one encoded segment, never as request path syntax.
                    if target.is_empty() || target.len() > 256 || target.chars().any(char::is_control) {
                        return Err(ContainerCredentialError::InvalidTarget);
                    }
                    server
                        .path_segments_mut()
                        .map_err(|_| ContainerCredentialError::InvalidDiscovery)?
                        .pop_if_empty()
                        .extend(["v1", "database", target, "identity"]);
                    resolve_name(server).await?
                }
            },
        };
        Ok((uri.clone(), target))
    }
}

pub(crate) struct Prepared {
    pub uri: Uri,
    pub target: String,
    pub credential: ContainerToken,
}

/// An in-memory, target-specific bearer credential. Debug output is redacted.
/// Expiry is enforced by the receiving host even if its bytes are copied.
pub struct ContainerToken {
    token: String,
    target: Identity,
    expiry: SystemTime,
    deadline: Instant,
}

impl ContainerToken {
    /// The token for an `Authorization: Bearer ...` header. Do not log or save it.
    pub fn as_str(&self) -> &str {
        &self.token
    }

    pub fn target(&self) -> Identity {
        self.target
    }

    pub fn expires_at(&self) -> SystemTime {
        self.expiry
    }

    pub fn remaining_lifetime(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl fmt::Debug for ContainerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContainerToken")
            .field("target", &self.target)
            .field("expires_at", &self.expiry)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenResponse {
    token: String,
    expires_unix_seconds: u64,
}

fn client_builder() -> reqwest::ClientBuilder {
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .referer(false)
        .http1_only()
        .pool_max_idle_per_host(0)
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
}

fn transport_error(error: reqwest::Error) -> ContainerCredentialError {
    if error.is_timeout() {
        ContainerCredentialError::Timeout
    } else {
        ContainerCredentialError::Transport
    }
}

async fn bounded_body(mut response: Response, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(ContainerCredentialError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn header_size(headers: &header::HeaderMap) -> usize {
    headers
        .iter()
        .map(|(key, value)| key.as_str().len() + value.as_bytes().len() + 4)
        .sum()
}

async fn resolve_name(uri: Url) -> Result<Identity> {
    tokio::time::timeout(REQUEST_TIMEOUT, async {
        let client = client_builder()
            .build()
            .map_err(|_| ContainerCredentialError::NameResolution)?;
        let response = client
            .get(uri)
            .send()
            .await
            .map_err(|_| ContainerCredentialError::NameResolution)?;
        if response.status() != reqwest::StatusCode::OK || header_size(response.headers()) > MAX_HEADERS {
            return Err(ContainerCredentialError::NameResolution);
        }
        let body = bounded_body(response, 64)
            .await
            .map_err(|_| ContainerCredentialError::NameResolution)?;
        parse_identity(std::str::from_utf8(&body).map_err(|_| ContainerCredentialError::NameResolution)?)
            .map_err(|_| ContainerCredentialError::NameResolution)
    })
    .await
    .map_err(|_| ContainerCredentialError::Timeout)?
}

fn parse_identity(value: &str) -> Result<Identity> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ContainerCredentialError::InvalidTarget);
    }
    Identity::from_hex(value).map_err(|_| ContainerCredentialError::InvalidTarget)
}

fn parse_url(value: &str) -> Result<Url> {
    if value.is_empty() || value.len() > MAX_URI || value.bytes().any(|byte| byte.is_ascii_control() || byte == b' ') {
        return Err(ContainerCredentialError::InvalidDiscovery);
    }
    let uri = Url::parse(value).map_err(|_| ContainerCredentialError::InvalidDiscovery)?;
    if !uri.username().is_empty() || uri.password().is_some() || uri.query().is_some() || uri.fragment().is_some() {
        return Err(ContainerCredentialError::InvalidDiscovery);
    }
    Ok(uri)
}

fn server_url(value: &str) -> Result<Url> {
    let mut uri = parse_url(value)?;
    match uri.scheme() {
        "http" | "https" => {}
        "ws" => uri
            .set_scheme("http")
            .map_err(|_| ContainerCredentialError::InvalidDiscovery)?,
        "wss" => uri
            .set_scheme("https")
            .map_err(|_| ContainerCredentialError::InvalidDiscovery)?,
        _ => return Err(ContainerCredentialError::InvalidDiscovery),
    }
    if uri.host_str().is_none() {
        return Err(ContainerCredentialError::InvalidDiscovery);
    }
    Ok(uri)
}

fn endpoint(value: &str) -> Result<Endpoint> {
    let uri = parse_url(value)?;
    match uri.scheme() {
        "http" => {
            // Parse the original authority as an IP, rejecting DNS aliases and
            // URL parser shortcuts such as hexadecimal/abbreviated IPv4.
            let original: Uri = value.parse().map_err(|_| ContainerCredentialError::InvalidDiscovery)?;
            let host = original.host().ok_or(ContainerCredentialError::InvalidDiscovery)?;
            let ip: IpAddr = host
                .trim_matches(['[', ']'])
                .parse()
                .map_err(|_| ContainerCredentialError::InvalidDiscovery)?;
            if !ip.is_loopback() || uri.path() != "/v1/credentials" {
                return Err(ContainerCredentialError::InvalidDiscovery);
            }
            Ok(Endpoint::Http(uri))
        }
        "unix" => {
            let path = value
                .strip_prefix("unix://")
                .ok_or(ContainerCredentialError::InvalidDiscovery)?;
            if uri.host_str().is_some()
                || uri.port().is_some()
                || !path.starts_with('/')
                || path.len() > 103
                || path.contains('%')
                || path.split('/').any(|part| part == "." || part == "..")
                || PathBuf::from(path)
                    .components()
                    .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
            {
                return Err(ContainerCredentialError::InvalidDiscovery);
            }
            #[cfg(unix)]
            return Ok(Endpoint::Unix(PathBuf::from(path)));
            #[cfg(not(unix))]
            return Err(ContainerCredentialError::UnsupportedTransport);
        }
        _ => Err(ContainerCredentialError::UnsupportedTransport),
    }
}

#[cfg(test)]
pub(crate) mod tests;
