//! Types and utilities for performing HTTP requests in [procedures](crate::procedure).
//!
//! Perform an HTTP request using methods on [`crate::ProcedureContext::http`],
//! which is of type [`HttpClient`].
//! The [`get`](HttpClient::get) helper can be used for simple `GET` requests,
//! while [`send`](HttpClient::send) allows more complex requests with headers, bodies and other methods.

use bytes::Bytes;
pub use http::{Request, Response};
pub use spacetimedb_lib::http::{Error, Timeout};

use crate::{
    rt::{read_bytes_source_as, read_bytes_source_into},
    IterBuf,
};
use spacetimedb_lib::{bsatn, http as st_http};

/// Allows performing HTTP requests via [`HttpClient::send`] and [`HttpClient::get`].
///
/// Access an `HttpClient` from within [procedures](crate::procedure)
/// via [the `http` field of the `ProcedureContext`](crate::ProcedureContext::http).
#[non_exhaustive]
pub struct HttpClient {}

impl HttpClient {
    /// Send the HTTP request `request` and wait for its response.
    ///
    /// For simple `GET` requests with no headers, use [`HttpClient::get`] instead.
    ///
    /// Include a [`Timeout`] in the [`Request::extensions`] via [`http::request::RequestBuilder::extension`]
    /// to impose a timeout on the request.
    /// All HTTP requests in SpacetimeDB are subject to a maximum timeout of 500 milliseconds.
    /// All other extensions in `request` are ignored.
    ///
    /// The returned [`Response`] may have a status code other than 200 OK.
    /// Callers should inspect [`Response::status`] to handle errors returned from the remote server.
    /// This method returns `Err(err)` only when a connection could not be initiated or was dropped,
    /// e.g. due to DNS resolution failure or an unresponsive server.
    ///
    /// # Example
    ///
    /// Send a `POST` request with the header `Content-Type: text/plain`, a string body,
    /// and a timeout of 100 milliseconds, then treat the response as a string and log it:
    ///
    /// ```norun
    /// # use spacetimedb::procedure;
    /// # use spacetimedb::http::{Request, Timeout};
    /// # use std::time::Duration;
    /// # #[procedure]
    /// # fn post_somewhere(ctx: &mut ProcedureContext) {
    /// let request = Request::builder()
    ///     .uri("https://some-remote-host.invalid/upload")
    ///     .method("POST")
    ///     .header("Content-Type", "text/plain")
    ///     // Set a timeout of 100 ms, further restricting the default timeout.
    ///     .extension(Timeout::from(Duration::from_millis(100)))
    ///     .body("This is the body of the HTTP request")
    ///     .expect("Building `Request` object failed");
    ///
    /// match ctx.http.send(request) {
    ///     Err(err) => {
    ///         log::error!("HTTP request failed: {err}");
    ///     },
    ///     Ok(response) => {
    ///         log::info!(
    ///             "Got response with status {}, body {}",
    ///             response.status(),
    ///             response.body().into_string_lossy(),
    ///         );
    ///     }
    /// }
    /// # }
    ///
    /// ```
    pub fn send<B: Into<Body>>(&self, request: Request<B>) -> Result<Response<Body>, Error> {
        let (request, body) = request.map(Into::into).into_parts();
        let request = st_http::Request::from(request);
        let request = bsatn::to_vec(&request).expect("Failed to BSATN-serialize `spacetimedb_lib::http::Request`");

        match spacetimedb_bindings_sys::procedure::http_request(&request, &body.into_bytes()) {
            Ok((response_source, body_source)) => {
                let response = read_bytes_source_as::<st_http::Response>(response_source);
                let response =
                    http::response::Parts::try_from(response).expect("Invalid http response returned from host");
                let mut buf = IterBuf::take();
                read_bytes_source_into(body_source, &mut buf);
                let body = Body::from_bytes(buf.clone());

                Ok(http::Response::from_parts(response, body))
            }
            Err(err_source) => {
                let error = read_bytes_source_as::<st_http::Error>(err_source);
                Err(error)
            }
        }
    }

    /// Send a `GET` request to `uri` with no headers and wait for the response.
    ///
    /// # Example
    ///
    /// Send a `GET` request, then treat the response as a string and log it:
    ///
    /// ```no_run
    /// # use spacetimedb::procedure;
    /// # #[procedure]
    /// # fn get_from_somewhere(ctx: &mut ProcedureContext) {
    /// match ctx.http.get("https://some-remote-host.invalid/download") {
    ///     Err(err) => {
    ///         log::error!("HTTP request failed: {err}");
    ///     }
    ///     Ok(response) => {
    ///         log::info!(
    ///             "Got response with status {}, body {}",
    ///             response.status(),
    ///             response.body().into_string_lossy(),
    ///         );
    ///     }
    /// }
    /// # }
    /// ```
    pub fn get(&self, uri: impl TryInto<http::Uri, Error: Into<http::Error>>) -> Result<Response<Body>, Error> {
        self.send(
            http::Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .map_err(|err| Error::from_display(&err))?,
        )
    }
}

/// Represents the body of an HTTP request or response.
pub struct Body {
    inner: BodyInner,
}

impl Body {
    /// Treat the body as a sequence of bytes.
    pub fn into_bytes(self) -> Bytes {
        match self.inner {
            BodyInner::Bytes(bytes) => bytes,
        }
    }

    /// Convert the body into a [`String`], erroring if it is not valid UTF-8.
    pub fn into_string(self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.into_bytes().into())
    }

    /// Convert the body into a [`String`], replacing invalid UTF-8 with
    /// `U+FFFD REPLACEMENT CHARACTER`, which looks like this: �.
    ///
    /// See [`String::from_utf8_lossy`] for more details on the conversion.
    pub fn into_string_lossy(self) -> String {
        self.into_string()
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    }

    /// Construct a `Body` consisting of `bytes`.
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Body {
        Body {
            inner: BodyInner::Bytes(bytes.into()),
        }
    }

    /// An empty body, suitable for a `GET` request.
    pub fn empty() -> Body {
        ().into()
    }

    /// Is `self` exactly zero bytes?
    pub fn is_empty(&self) -> bool {
        match &self.inner {
            BodyInner::Bytes(bytes) => bytes.is_empty(),
        }
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::empty()
    }
}

macro_rules! impl_body_from_bytes {
    ($bytes:ident : $t:ty => $conv:expr) => {
        impl From<$t> for Body {
            fn from($bytes: $t) -> Body {
                Body::from_bytes($conv)
            }
        }
    };
    ($t:ty) => {
        impl_body_from_bytes!(bytes : $t => bytes);
    };
}

impl_body_from_bytes!(String);
impl_body_from_bytes!(Vec<u8>);
impl_body_from_bytes!(Box<[u8]>);
impl_body_from_bytes!(&'static [u8]);
impl_body_from_bytes!(&'static str);
impl_body_from_bytes!(_unit: () => Bytes::new());

enum BodyInner {
    Bytes(Bytes),
}
