//! Uploading and downloading a database's files.
//!
//! Files travel over HTTP, beside the WebSocket that carries subscriptions and reducer calls.
//! An upload returns a [`FileRef`] once the file is durable, and the client then passes it to a
//! reducer, which stores it in a table like any other value:
//!
//! ```ignore
//! let options = UploadOptions { content_type: Some("image/png".into()), ..Default::default() };
//! let avatar = conn.files().upload(bytes, options).await?;
//! conn.reducers.set_avatar(avatar)?;
//! ```
//!
//! Rows that hold a `FileRef` reach subscribers as usual, and any of them can download the file.
//! Clients can't create files from bytes, copy them, or delete them; only modules can.

use crate::db_connection::SharedCell;
use crate::error::InternalError;
use bytes::Bytes;
use http::uri::{Scheme, Uri};
use spacetimedb_lib::file::FileRef;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

pub use url::Url;

/// A handle for uploading and downloading the files of the database a connection is connected to.
///
/// Get one with `DbConnection::files`. It's cheap to clone, and it authenticates as the connection
/// does, including with the token the server issued if the connection was anonymous.
///
/// Browsers can build URLs, but uploading and downloading aren't supported there yet.
#[derive(Clone)]
pub struct Files {
    /// `http(s)://host/v1/database/:name_or_identity/files`.
    base: Arc<str>,
    token: SharedCell<Option<String>>,
    #[cfg(not(feature = "browser"))]
    client: reqwest::Client,
}

/// How to serve a file from its URL.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServeOptions {
    /// Serve the file as an attachment with this name, so browsers download it.
    pub download_name: Option<String>,
    /// Serve the file with this content type instead of the one it was stored with.
    pub content_type: Option<String>,
}

/// How to upload a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UploadOptions {
    /// The file's content type, which the gateway serves it with. Nothing verifies it.
    pub content_type: Option<String>,
    /// Names this upload across retries. The SDK generates one when absent;
    /// pass one explicitly to recover an upload after a restart.
    pub idempotency_key: Option<String>,
}

/// The bytes of an upload.
///
/// Uploads are retried, so the body is held in memory rather than streamed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadBody(Bytes);

impl From<Bytes> for UploadBody {
    fn from(bytes: Bytes) -> Self {
        Self(bytes)
    }
}

impl From<Vec<u8>> for UploadBody {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes.into())
    }
}

impl From<&'static [u8]> for UploadBody {
    fn from(bytes: &'static [u8]) -> Self {
        Self(Bytes::from_static(bytes))
    }
}

impl From<String> for UploadBody {
    fn from(text: String) -> Self {
        Self(text.into())
    }
}

impl From<&'static str> for UploadBody {
    fn from(text: &'static str) -> Self {
        Self(Bytes::from_static(text.as_bytes()))
    }
}

/// A file's bytes as they arrive.
pub type ByteStream = Pin<Box<dyn futures::Stream<Item = Result<Bytes, FilesError>> + Send>>;

/// Why a file request failed.
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FilesError {
    /// The request didn't complete, for example because the connection dropped.
    #[error("the file request failed: {message}")]
    Request { message: String },
    /// The server refused the request.
    #[error("the server responded {status}: {message}")]
    Status { status: u16, message: String },
    /// The server's response couldn't be understood.
    #[error("the server's file response was malformed: {message}")]
    Malformed { message: String },
}

/// Why an upload failed.
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UploadError {
    /// The module's `allow_upload` reducer refused the upload.
    #[error("the upload was refused: {message}")]
    Rejected { message: String },
    /// The connection has no identity to upload as.
    #[error("the upload needs an identity: {message}")]
    Unauthorized { message: String },
    /// The file is too large, or the database already has too many uploads in progress.
    #[error("the upload is too large or the database is too busy: {message}")]
    TooLarge { message: String },
    /// The database is rate limiting uploads.
    #[error("the upload was rate limited: {message}")]
    RateLimited {
        retry_after: Option<Duration>,
        message: String,
    },
    #[error(transparent)]
    Files(#[from] FilesError),
}

impl Files {
    pub(crate) fn new(host: &Uri, database_name: &str, token: Option<String>) -> crate::Result<Self> {
        let scheme = match host.scheme_str() {
            Some("ws" | "http") | None => Scheme::HTTP,
            Some("wss" | "https") => Scheme::HTTPS,
            Some(other) => {
                return Err(InternalError::new(format!("unknown URI scheme `{other}`")).into());
            }
        };
        let authority = host
            .authority()
            .ok_or_else(|| InternalError::new(format!("the URI `{host}` has no host")))?;
        let path = host.path().trim_end_matches('/');
        let base = format!("{scheme}://{authority}{path}/v1/database/{database_name}/files");
        Url::parse(&base).map_err(|e| InternalError::new(format!("`{base}` is not a valid URL")).with_cause(e))?;
        Ok(Self {
            base: base.into(),
            token: Arc::new(StdMutex::new(token)),
            #[cfg(not(feature = "browser"))]
            client: reqwest::Client::new(),
        })
    }

    /// Remembers the token the server issued an anonymous connection.
    pub(crate) fn learn_token(&self, token: &str) {
        let mut stored = self.token.lock().unwrap();
        if stored.is_none() {
            *stored = Some(token.to_owned());
        }
    }

    /// The URL from which anyone holding `file_ref` can download the file, for example in a browser.
    ///
    /// This is computed locally. The reference carries the file's read secret, so the URL needs no
    /// other credentials. It points to the database's HTTP API, which serves the file from object
    /// storage, not to object storage itself.
    pub fn url(&self, file_ref: FileRef) -> Url {
        // `base` was checked when the handle was made, and a reference is 64 hex digits.
        Url::parse(&format!("{}/{file_ref}", self.base)).expect("a file URL is a valid URL")
    }

    /// The file's URL with presentation options, such as a download name or content type.
    pub fn url_with(&self, file_ref: FileRef, options: ServeOptions) -> Url {
        let mut url = self.url(file_ref);
        {
            let mut query = url.query_pairs_mut();
            if let Some(name) = &options.download_name {
                query.append_pair("download", name);
            }
            if let Some(content_type) = &options.content_type {
                query.append_pair("type", content_type);
            }
        }
        if url.query() == Some("") {
            url.set_query(None);
        }
        url
    }
}

/// Requests over HTTP, which browsers don't support yet.
#[cfg(not(feature = "browser"))]
mod native {
    use super::*;
    use futures::TryStreamExt as _;
    use spacetimedb_lib::file::FileMetadata;
    use spacetimedb_lib::{Hash, Timestamp};
    use std::ops::Range;

    /// How many times an upload is attempted before its error is returned. Every attempt carries
    /// the same idempotency key, so retries never create a second file.
    const UPLOAD_ATTEMPTS_MAX: u32 = 4;

    /// The pause before the first retry, doubled before each later one.
    const UPLOAD_RETRY_DELAY_INITIAL: Duration = Duration::from_millis(250);

    fn request_failed(err: reqwest::Error) -> FilesError {
        FilesError::Request {
            message: err.to_string(),
        }
    }

    fn malformed(what: &str) -> FilesError {
        FilesError::Malformed {
            message: format!("bad or missing `{what}`"),
        }
    }

    /// The status and explanation of a failed response.
    async fn failure(response: reqwest::Response) -> (u16, Option<Duration>, String) {
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(http::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs);
        (status, retry_after, response.text().await.unwrap_or_default())
    }

    fn field<'a>(json: &'a serde_json::Value, name: &str) -> Result<&'a serde_json::Value, FilesError> {
        json.get(name).ok_or_else(|| malformed(name))
    }

    fn parse_file_ref(json: &serde_json::Value) -> Result<FileRef, FilesError> {
        field(json, "file_ref")?
            .as_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| malformed("file_ref"))
    }

    /// A key that names one upload across its retries.
    fn new_idempotency_key() -> String {
        format!("{:032x}", rand::random::<u128>())
    }

    impl Files {
        fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
            match &*self.token.lock().unwrap() {
                Some(token) => request.bearer_auth(token),
                None => request,
            }
        }

        /// Uploads a new file, and returns its reference once its bytes are verified and durable
        /// and its `st_file` record is stored.
        ///
        /// The module's `allow_upload` lifecycle reducer decides whether to accept it, before any
        /// bytes are sent. Failed attempts are retried with the same idempotency key, so they never
        /// create a second file.
        pub async fn upload(
            &self,
            body: impl Into<UploadBody>,
            options: UploadOptions,
        ) -> Result<FileRef, UploadError> {
            let UploadBody(bytes) = body.into();
            let key = options.idempotency_key.unwrap_or_else(new_idempotency_key);
            let mut delay = UPLOAD_RETRY_DELAY_INITIAL;
            let mut attempt = 1;
            loop {
                match self.upload_once(&bytes, options.content_type.as_deref(), &key).await {
                    Err(UploadAttempt::Retry(error)) if attempt < UPLOAD_ATTEMPTS_MAX => {
                        log::warn!("upload attempt {attempt} failed, retrying in {delay:?}: {error}");
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                        attempt += 1;
                    }
                    Err(UploadAttempt::Retry(error) | UploadAttempt::Fail(error)) => return Err(error),
                    Ok(file_ref) => return Ok(file_ref),
                }
            }
        }

        async fn upload_once(
            &self,
            bytes: &Bytes,
            content_type: Option<&str>,
            key: &str,
        ) -> Result<FileRef, UploadAttempt> {
            let mut request = self
                .client
                .post(&*self.base)
                .header("idempotency-key", key)
                .body(bytes.clone());
            if let Some(content_type) = content_type {
                request = request.header(http::header::CONTENT_TYPE, content_type);
            }
            let response = self
                .authorize(request)
                .send()
                .await
                .map_err(|e| UploadAttempt::Retry(request_failed(e).into()))?;
            if response.status().is_success() {
                let json: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| UploadAttempt::Retry(request_failed(e).into()))?;
                return parse_file_ref(&json).map_err(|e| UploadAttempt::Fail(e.into()));
            }
            let (status, retry_after, message) = failure(response).await;
            Err(match status {
                401 => UploadAttempt::Fail(UploadError::Unauthorized { message }),
                403 => UploadAttempt::Fail(UploadError::Rejected { message }),
                413 => UploadAttempt::Fail(UploadError::TooLarge { message }),
                429 => UploadAttempt::Fail(UploadError::RateLimited { retry_after, message }),
                // An earlier attempt with this key is still running, or the server had a problem.
                409 | 500..=599 => UploadAttempt::Retry(FilesError::Status { status, message }.into()),
                _ => UploadAttempt::Fail(FilesError::Status { status, message }.into()),
            })
        }

        /// Returns the file's bytes, or the `range` of them, including after the file is deleted.
        ///
        /// `None` if the reference is unknown or its secret is wrong, or the bytes have been reclaimed.
        pub async fn download(
            &self,
            file_ref: FileRef,
            range: Option<Range<u64>>,
        ) -> Result<Option<ByteStream>, FilesError> {
            let mut request = self.client.get(self.url(file_ref));
            if let Some(range) = range {
                if range.is_empty() {
                    return Ok(Some(Box::pin(futures::stream::empty())));
                }
                request = request.header(http::header::RANGE, format!("bytes={}-{}", range.start, range.end - 1));
            }
            let response = request.send().await.map_err(request_failed)?;
            if response.status() == http::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                let (status, _, message) = failure(response).await;
                return Err(FilesError::Status { status, message });
            }
            Ok(Some(Box::pin(response.bytes_stream().map_err(request_failed))))
        }

        /// The file's metadata, or `None` once it has been deleted, or if it never existed.
        ///
        /// Unlike [`Self::download`], it reads the live record, so it returns `None` as soon as the
        /// deletion commits, even while the bytes remain downloadable.
        pub async fn metadata(&self, file_ref: FileRef) -> Result<Option<FileMetadata>, FilesError> {
            let url = format!("{}/metadata", self.url(file_ref));
            let response = self.client.get(url).send().await.map_err(request_failed)?;
            if response.status() == http::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                let (status, _, message) = failure(response).await;
                return Err(FilesError::Status { status, message });
            }
            let json: serde_json::Value = response.json().await.map_err(request_failed)?;
            let content_hash = field(&json, "content_hash")?
                .as_str()
                .and_then(|s| Hash::from_hex(s).ok())
                .ok_or_else(|| malformed("content_hash"))?;
            let size = field(&json, "size")?.as_u64().ok_or_else(|| malformed("size"))?;
            let created_at = field(&json, "created_at")?
                .as_str()
                .and_then(|s| Timestamp::parse_from_rfc3339(s).ok())
                .ok_or_else(|| malformed("created_at"))?;
            let content_type = match field(&json, "content_type")? {
                serde_json::Value::Null => None,
                value => Some(value.as_str().ok_or_else(|| malformed("content_type"))?.to_owned()),
            };
            Ok(Some(FileMetadata::new(
                parse_file_ref(&json)?,
                content_hash,
                size,
                created_at,
                content_type,
            )))
        }
    }

    /// The outcome of one failed upload attempt.
    enum UploadAttempt {
        /// Worth retrying with the same key.
        Retry(UploadError),
        Fail(UploadError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(uri: &str) -> Files {
        Files::new(&uri.parse().unwrap(), "quickstart", None).unwrap()
    }

    #[test]
    fn urls_follow_the_connection_uri() {
        let base = |uri: &str| files(uri).base.to_string();
        assert_eq!(
            base("ws://localhost:3000"),
            "http://localhost:3000/v1/database/quickstart/files"
        );
        assert_eq!(
            base("https://example.com/"),
            "https://example.com/v1/database/quickstart/files"
        );
        assert_eq!(
            base("wss://example.com/prefix"),
            "https://example.com/prefix/v1/database/quickstart/files"
        );
        assert_eq!(
            base("localhost:3000"),
            "http://localhost:3000/v1/database/quickstart/files"
        );
    }

    #[test]
    fn urls_carry_serving_options() {
        let files = files("ws://localhost:3000");
        let file_ref = FileRef::new(spacetimedb_lib::FileId(1), 2);
        let plain = files.url(file_ref);
        assert_eq!(plain.query(), None);
        assert!(plain.path().ends_with(&file_ref.to_string()));
        let options = ServeOptions {
            download_name: Some("cat photo.png".into()),
            content_type: Some("image/png".into()),
        };
        let url = files.url_with(file_ref, options);
        assert_eq!(url.query(), Some("download=cat+photo.png&type=image%2Fpng"));
        assert_eq!(files.url_with(file_ref, ServeOptions::default()), plain);
    }
}
