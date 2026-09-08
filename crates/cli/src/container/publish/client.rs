//! Ordinary publisher transport. Credentials are held in memory and never
//! redirected, inherited from a proxy, or sent to an unapproved artifact URL.
use anyhow::{bail, ensure, Context, Result};
use reqwest::{header::HeaderValue, Client, Method, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use spacetimedb_client_api_messages::name::{DomainName, PrePublishResult, SetDomainsResult};
use spacetimedb_lib::{container::OciDigest, deployment::api::*, Identity, Uuid};
use std::time::Duration;

pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
// A submission composes the 120s schema worker, 300s image verification,
// two independently bounded 300s artifact writes, and read/pin/control calls.
// This caller deadline does not extend those server resource or credential
// bounds. Cancellation/timeout leaves the exact operation in its journal.
const COORDINATOR_REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const UPLOAD_CHUNK_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("{action} returned HTTP {status}")]
pub struct HttpFailure {
    pub action: &'static str,
    pub status: StatusCode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadKind {
    Manifest,
    Config,
    Layer,
    Module,
}
impl UploadKind {
    fn header(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Config => "config",
            Self::Layer => "layer",
            Self::Module => "module",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRef {
    pub digest: OciDigest,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadStatus {
    pub id: uuid::Uuid,
    pub object: ObjectRef,
    pub offset: u64,
    pub expires_at: u64,
    pub complete: bool,
}
impl UploadStatus {
    pub fn validate(&self, object: ObjectRef, id: Option<uuid::Uuid>) -> Result<()> {
        ensure!(
            self.id.get_version_num() == 4 && id.is_none_or(|id| self.id == id),
            "artifact upload session changed"
        );
        ensure!(
            self.object == object && self.offset <= object.size && (!self.complete || self.offset == object.size),
            "artifact upload descriptor or offset changed"
        );
        Ok(())
    }
}

pub struct ArtifactEndpoint(Url);
impl ArtifactEndpoint {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

pub struct PublisherClient {
    http: Client,
    server: Url,
    authorization: HeaderValue,
}
impl PublisherClient {
    pub fn new(server: &str, mut authorization: HeaderValue) -> Result<Self> {
        let server = endpoint(server)?;
        ensure!(
            authorization.as_bytes().starts_with(b"Bearer "),
            "managed publication requires ordinary Bearer authentication"
        );
        authorization.set_sensitive(true);
        let http = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            server,
            authorization,
        })
    }
    pub fn server(&self) -> &Url {
        &self.server
    }
    fn request(&self, method: Method, url: Url) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
    }
    /// A response cannot grant itself permission to receive this credential.
    /// Same-origin paths are accepted; another origin requires an explicit URL.
    pub fn artifact_endpoint(&self, advertised: &str, approved: Option<&str>) -> Result<ArtifactEndpoint> {
        let artifact = endpoint(advertised)?;
        if artifact.origin() != self.server.origin() {
            let approved=approved.context("artifact service uses another origin; pass --artifact-endpoint with its exact trusted URL before sending publisher credentials")?;
            ensure!(
                endpoint(approved)? == artifact,
                "advertised artifact endpoint differs from the explicitly approved endpoint"
            );
        } else if let Some(approved) = approved {
            ensure!(
                endpoint(approved)? == artifact,
                "advertised artifact endpoint differs from the explicitly approved endpoint"
            );
        }
        Ok(ArtifactEndpoint(artifact))
    }
    pub async fn capabilities(&self) -> Result<Option<PublicationCapabilities>> {
        let response = self
            .http
            .get(route(&self.server, &["v1", "containers", "capabilities"])?)
            .send()
            .await?;
        optional_json(response, "publication capabilities").await
    }
    pub async fn permission(&self) -> Result<PublishPermission> {
        json(
            self.request(
                Method::GET,
                route(&self.server, &["v1", "containers", "publish-permission"])?,
            )
            .send()
            .await?,
            "publication permission",
        )
        .await
    }
    pub async fn deployment(&self, name: &str) -> Result<Option<DeploymentStatus>> {
        optional_json(
            self.request(
                Method::GET,
                route(&self.server, &["v1", "database", name, "deployment"])?,
            )
            .send()
            .await?,
            "deployment inspection",
        )
        .await
    }
    pub async fn reserve(&self, request: &ReserveDatabaseRequest) -> Result<DatabaseReservation> {
        json(
            self.request(
                Method::POST,
                route(&self.server, &["v1", "containers", "reservations"])?,
            )
            .json(request)
            .send()
            .await?,
            "database reservation",
        )
        .await
    }
    pub async fn submit(&self, database: Identity, request: &PublishRequest) -> Result<PublicationStatus> {
        let bytes = serde_json::to_vec(request)?;
        self.submit_bytes(database, &bytes).await
    }
    /// Resume sends these same immutable bytes, without reconstructing a request
    /// from a changed tag, project configuration, or current deployment.
    pub async fn submit_bytes(&self, database: Identity, bytes: &[u8]) -> Result<PublicationStatus> {
        ensure!(bytes.len() <= 256 * 1024, "publication HTTP body exceeds 256 KiB");
        json(
            self.request(
                Method::PUT,
                route(&self.server, &["v1", "database", &database.to_hex(), "deployment"])?,
            )
            .timeout(COORDINATOR_REQUEST_TIMEOUT)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes.to_vec())
            .send()
            .await?,
            "publication submission",
        )
        .await
    }
    pub async fn status(&self, database: Identity, operation: Uuid) -> Result<Option<PublicationStatus>> {
        optional_json(
            self.request(
                Method::GET,
                route(
                    &self.server,
                    &[
                        "v1",
                        "database",
                        &database.to_hex(),
                        "deployment",
                        "operations",
                        &operation.to_string(),
                    ],
                )?,
            )
            .send()
            .await?,
            "publication status",
        )
        .await
    }
    pub async fn preflight(&self, database: Identity, kind: &str, bytes: &[u8]) -> Result<PrePublishResult> {
        json(
            self.request(
                Method::POST,
                route(&self.server, &["v1", "database", &database.to_hex(), "pre_publish"])?,
            )
            .query(&[("host_type", kind), ("pretty_print_style", "NoColor")])
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await?,
            "module migration preflight",
        )
        .await
    }
    pub async fn set_name(&self, database: Identity, name: &str) -> Result<()> {
        let name: DomainName = name.parse()?;
        let result: SetDomainsResult = json(
            self.request(
                Method::PUT,
                route(&self.server, &["v1", "database", &database.to_hex(), "names"])?,
            )
            .json(&[name])
            .send()
            .await?,
            "database naming",
        )
        .await?;
        ensure!(
            matches!(result, SetDomainsResult::Success),
            "database was created, but assigning its requested name was not confirmed"
        );
        Ok(())
    }
    pub async fn begin_upload(
        &self,
        artifact: &ArtifactEndpoint,
        database: Identity,
        kind: UploadKind,
        object: ObjectRef,
    ) -> Result<UploadStatus> {
        let status: UploadStatus = json(
            self.request(
                Method::POST,
                route(&artifact.0, &["v1", "databases", &database.to_hex(), "uploads"])?,
            )
            .header("x-spacetimedb-artifact-kind", kind.header())
            .json(&serde_json::json!({"kind":kind,"object":object}))
            .send()
            .await?,
            "artifact upload creation",
        )
        .await?;
        status.validate(object, None)?;
        Ok(status)
    }
    pub async fn upload_status(
        &self,
        artifact: &ArtifactEndpoint,
        database: Identity,
        id: uuid::Uuid,
        object: ObjectRef,
    ) -> Result<UploadStatus> {
        let status: UploadStatus = json(
            self.request(
                Method::GET,
                route(
                    &artifact.0,
                    &["v1", "databases", &database.to_hex(), "uploads", &id.to_string()],
                )?,
            )
            .send()
            .await?,
            "artifact upload status",
        )
        .await?;
        status.validate(object, Some(id))?;
        Ok(status)
    }
    pub async fn append_upload(
        &self,
        artifact: &ArtifactEndpoint,
        database: Identity,
        status: &UploadStatus,
        chunk: Vec<u8>,
    ) -> Result<UploadStatus> {
        ensure!(
            !chunk.is_empty()
                && chunk.len() <= UPLOAD_CHUNK_BYTES
                && status
                    .offset
                    .checked_add(chunk.len() as u64)
                    .is_some_and(|end| end <= status.object.size),
            "invalid artifact chunk"
        );
        let next: UploadStatus = json(
            self.request(
                Method::PATCH,
                route(
                    &artifact.0,
                    &["v1", "databases", &database.to_hex(), "uploads", &status.id.to_string()],
                )?,
            )
            .query(&[("offset", status.offset)])
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(chunk)
            .send()
            .await?,
            "artifact upload append",
        )
        .await?;
        next.validate(status.object, Some(status.id))?;
        Ok(next)
    }
    pub async fn complete_upload(
        &self,
        artifact: &ArtifactEndpoint,
        database: Identity,
        status: &UploadStatus,
    ) -> Result<UploadStatus> {
        let completed: ObjectRef = json(
            self.request(
                Method::POST,
                route(
                    &artifact.0,
                    &[
                        "v1",
                        "databases",
                        &database.to_hex(),
                        "uploads",
                        &status.id.to_string(),
                        "complete",
                    ],
                )?,
            )
            .send()
            .await?,
            "artifact upload completion",
        )
        .await?;
        // The completion endpoint confirms the immutable object, rather than
        // returning session status. The route already binds the original UUID;
        // retain that session only after checking the exact digest and size.
        ensure!(completed == status.object, "artifact completion descriptor changed");
        let mut next = status.clone();
        next.offset = completed.size;
        next.complete = true;
        next.validate(status.object, Some(status.id))?;
        Ok(next)
    }
}

pub(crate) fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}
pub fn endpoint(value: &str) -> Result<Url> {
    let mut url = Url::parse(value).context("resolved server URL must use HTTP(S)")?;
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none(),
        "endpoint must not contain credentials, query, or fragment"
    );
    let local = is_loopback(&url);
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && local),
        "use HTTPS, or HTTP for a local loopback server"
    );
    ensure!(url.host_str().is_some(), "endpoint host is missing");
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}
fn route(base: &Url, segments: &[&str]) -> Result<Url> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid base URL"))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}
async fn optional_json<T: DeserializeOwned>(response: reqwest::Response, action: &'static str) -> Result<Option<T>> {
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    json(response, action).await.map(Some)
}
async fn json<T: DeserializeOwned>(mut response: reqwest::Response, action: &'static str) -> Result<T> {
    if !response.status().is_success() {
        return Err(HttpFailure {
            action,
            status: response.status(),
        }
        .into());
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        bail!("{action} response exceeds its bound");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            chunk.len() <= MAX_RESPONSE_BYTES.saturating_sub(bytes.len()),
            "{action} response exceeds its bound"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {action} response"))
}
