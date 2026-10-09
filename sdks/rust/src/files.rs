//! Uploading and downloading a database's files.
//!
//! Files travel over HTTP, beside the WebSocket that carries subscriptions and reducer calls.
//! An upload returns a [`FileRef`] once the file is durable, and the client then passes it to a
//! reducer, which stores it in a table like any other value:
//!
//! ```ignore
//! let uploaded = conn.files().upload("image/png", bytes).await?;
//! conn.reducers.set_avatar(uploaded.file_ref)?;
//! ```
//!
//! Rows that hold a `FileRef` reach subscribers as usual, and any of them can download the file.

use crate::db_connection::SharedCell;
use crate::error::InternalError;
use http::uri::{Scheme, Uri};
use spacetimedb_lib::file::FileRef;
use spacetimedb_lib::Hash;
use std::sync::{Arc, Mutex as StdMutex};

/// A handle for uploading and downloading the files of the database a connection is connected to.
///
/// Get one with `DbConnection::files`. It's cheap to clone, and it authenticates as the connection
/// does, including with the token the server issued if the connection was anonymous.
///
/// Uploading and downloading aren't yet supported in browsers.
#[derive(Clone)]
pub struct Files {
    /// `http(s)://host/v1/database/:name_or_identity/files`.
    base: Arc<str>,
    token: SharedCell<Option<String>>,
    #[cfg(not(feature = "browser"))]
    client: reqwest::Client,
}

/// What an upload created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadedFile {
    /// The reference to pass to reducers that store the file.
    pub file_ref: FileRef,
    pub size: u64,
    /// The BLAKE3 hash of the file's contents.
    pub content_hash: Hash,
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

    /// The URL from which anyone holding `file` can download it, for example in a browser.
    ///
    /// The reference carries the file's read secret, so the URL needs no other credentials.
    pub fn url(&self, file: FileRef) -> String {
        format!("{}/{file}", self.base)
    }
}

/// Requests over HTTP, which browsers don't support yet.
#[cfg(not(feature = "browser"))]
mod native {
    use super::{Files, UploadedFile};
    use crate::error::{Error, InternalError};
    use spacetimedb_lib::file::{FileMetadata, FileRef};
    use spacetimedb_lib::{Hash, Timestamp};

    /// Turns a failed request into an [`Error::FileRequest`].
    fn request_failed(err: reqwest::Error) -> Error {
        Error::FileRequest {
            status: err.status().map(|s| s.as_u16()),
            message: err.to_string(),
        }
    }

    /// Fails with the server's explanation unless `response` succeeded.
    async fn check(response: reqwest::Response) -> crate::Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let message = response.text().await.unwrap_or_default();
        Err(Error::FileRequest {
            status: Some(status.as_u16()),
            message,
        })
    }

    fn malformed(what: &str) -> Error {
        InternalError::new(format!("the server's file response has a malformed `{what}`")).into()
    }

    fn field<'a>(json: &'a serde_json::Value, name: &str) -> crate::Result<&'a serde_json::Value> {
        json.get(name).ok_or_else(|| malformed(name))
    }

    fn parse_file_ref(json: &serde_json::Value) -> crate::Result<FileRef> {
        field(json, "file_ref")?
            .as_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| malformed("file_ref"))
    }

    fn parse_size(json: &serde_json::Value) -> crate::Result<u64> {
        field(json, "size")?.as_u64().ok_or_else(|| malformed("size"))
    }

    fn parse_hash(json: &serde_json::Value) -> crate::Result<Hash> {
        field(json, "content_hash")?
            .as_str()
            .and_then(|s| Hash::from_hex(s).ok())
            .ok_or_else(|| malformed("content_hash"))
    }

    impl Files {
        fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
            match &*self.token.lock().unwrap() {
                Some(token) => request.bearer_auth(token),
                None => request,
            }
        }

        /// Uploads `bytes` as a new file, and returns once the file is durable.
        ///
        /// The module's `allow_upload` lifecycle reducer decides whether to accept it.
        /// Nothing checks that `content_type` describes the contents.
        pub async fn upload(&self, content_type: &str, bytes: impl Into<bytes::Bytes>) -> crate::Result<UploadedFile> {
            self.upload_with(content_type, bytes.into(), None).await
        }

        /// Like [`Self::upload`], but safe to retry: every attempt with the same `idempotency_key`
        /// creates at most one file, and they all return it.
        ///
        /// The server remembers keys for a day. It requires the connection to have an identity.
        pub async fn upload_idempotent(
            &self,
            content_type: &str,
            bytes: impl Into<bytes::Bytes>,
            idempotency_key: &str,
        ) -> crate::Result<UploadedFile> {
            self.upload_with(content_type, bytes.into(), Some(idempotency_key))
                .await
        }

        async fn upload_with(
            &self,
            content_type: &str,
            bytes: bytes::Bytes,
            idempotency_key: Option<&str>,
        ) -> crate::Result<UploadedFile> {
            let mut request = self
                .client
                .post(&*self.base)
                .header(http::header::CONTENT_TYPE, content_type)
                .body(bytes);
            if let Some(key) = idempotency_key {
                request = request.header("idempotency-key", key);
            }
            let response = check(self.authorize(request).send().await.map_err(request_failed)?).await?;
            let json: serde_json::Value = response.json().await.map_err(request_failed)?;
            Ok(UploadedFile {
                file_ref: parse_file_ref(&json)?,
                size: parse_size(&json)?,
                content_hash: parse_hash(&json)?,
            })
        }

        /// Downloads the whole file.
        pub async fn download(&self, file: FileRef) -> crate::Result<bytes::Bytes> {
            let response = self.client.get(self.url(file)).send().await.map_err(request_failed)?;
            check(response).await?.bytes().await.map_err(request_failed)
        }

        /// Downloads the bytes of the file in `range`, which must lie within the file.
        pub async fn download_range(&self, file: FileRef, range: std::ops::Range<u64>) -> crate::Result<bytes::Bytes> {
            if range.is_empty() {
                return Ok(bytes::Bytes::new());
            }
            let response = self
                .client
                .get(self.url(file))
                .header(http::header::RANGE, format!("bytes={}-{}", range.start, range.end - 1))
                .send()
                .await
                .map_err(request_failed)?;
            check(response).await?.bytes().await.map_err(request_failed)
        }

        /// Returns the file's metadata, or an error with status 404 once the file is deleted.
        pub async fn metadata(&self, file: FileRef) -> crate::Result<FileMetadata> {
            let response = self
                .client
                .get(format!("{}/metadata", self.url(file)))
                .send()
                .await
                .map_err(request_failed)?;
            let json: serde_json::Value = check(response).await?.json().await.map_err(request_failed)?;
            let created_at = field(&json, "created_at")?
                .as_str()
                .and_then(|s| Timestamp::parse_from_rfc3339(s).ok())
                .ok_or_else(|| malformed("created_at"))?;
            let content_type = match field(&json, "content_type")? {
                serde_json::Value::Null => None,
                value => Some(value.as_str().ok_or_else(|| malformed("content_type"))?.to_owned()),
            };
            Ok(FileMetadata {
                file_ref: parse_file_ref(&json)?,
                content_hash: parse_hash(&json)?,
                size: parse_size(&json)?,
                created_at,
                content_type,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(uri: &str) -> String {
        Files::new(&uri.parse().unwrap(), "quickstart", None)
            .unwrap()
            .base
            .to_string()
    }

    #[test]
    fn urls_follow_the_connection_uri() {
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
}
