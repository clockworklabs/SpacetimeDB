//! `SpacetimeType`-ified HTTP request, response and error types,
//! for use in the procedure HTTP API.
//!
//! The types here are all mirrors of various types within the [`http`] crate.
//! That crate's types don't have stable representations or `pub`lic interiors,
//! so we're forced to define our own representation for the SATS serialization.
//! These types are that representation.
//!
//! Users aren't intended to interact with these types,
//! except [`Timeout`] and [`Error`], which are re-exported from the `bindings` crate.
//! Our user-facing APIs should use the [`http`] crate's types directly, and convert to and from these types internally.
//!
//! These types are used in BSATN encoding for interchange between the SpacetimeDB host
//! and guest WASM modules in the `procedure_http_request` ABI call.
//! For that reason, the layout of these types must not change.
//! Because we want, to the extent possible,
//! to support both (old host, new guest) and (new host, old guest) pairings,
//! we can't meaningfully make these types extensible, even with tricks like version enum wrappers.
//! Instead, if/when we want to add new functionality which requires sending additional information,
//! we'll define a new versioned ABI call which uses new types for interchange.

use spacetimedb_sats::{time_duration::TimeDuration, SpacetimeType};

/// Represents an HTTP request which can be made from a procedure running in a SpacetimeDB database.
///
/// Construct instances of this type by converting from [`http::Request`].
/// Represents an HTTP request which can be made from a procedure running in a SpacetimeDB database.
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct Request {
    method: Method,
    headers: Headers,
    timeout: Option<Timeout>,
    /// A valid URI, sourced from an already-validated [`http::Uri`].
    uri: String,
    version: Version,
}

impl From<http::request::Parts> for Request {
    fn from(parts: http::request::Parts) -> Request {
        let http::request::Parts {
            method,
            uri,
            version,
            headers,
            mut extensions,
            ..
        } = parts;

        let timeout = extensions.remove::<Timeout>();
        if !extensions.is_empty() {
            log::warn!("Converting HTTP `Request` with unrecognized extensions");
        }
        Request {
            method: method.into(),
            headers: headers.into(),
            timeout,
            uri: uri.to_string(),
            version: version.into(),
        }
    }
}

impl TryFrom<Request> for http::request::Parts {
    type Error = http::Error;
    fn try_from(req: Request) -> http::Result<http::request::Parts> {
        let Request {
            method,
            headers,
            timeout,
            uri,
            version,
        } = req;
        let (mut request, ()) = http::Request::new(()).into_parts();
        request.method = method.into();
        request.uri = uri.try_into()?;
        request.version = version.into();
        request.headers = headers.try_into()?;

        if let Some(timeout) = timeout {
            request.extensions.insert(timeout);
        }

        Ok(request)
    }
}

/// An HTTP extension to specify a timeout for requests made by a procedure running in a SpacetimeDB database.
///
/// Pass an instance of this type to [`http::request::Builder::extension`] to set a timeout on a request.
///
/// This timeout applies to the entire request,
/// from when the headers are first sent to when the response body is fully downloaded.
/// This is sometimes called a total timeout, the sum of the connect timeout and the read timeout.
#[derive(Clone, SpacetimeType, Copy, PartialEq, Eq)]
#[sats(crate = crate)]
pub struct Timeout {
    pub timeout: TimeDuration,
}

impl From<TimeDuration> for Timeout {
    fn from(timeout: TimeDuration) -> Timeout {
        Timeout { timeout }
    }
}

impl From<Timeout> for TimeDuration {
    fn from(Timeout { timeout }: Timeout) -> TimeDuration {
        timeout
    }
}

/// Represents an HTTP method.
#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
    Extension(String),
}

impl Method {
    pub const GET: Method = Method::Get;
    pub const HEAD: Method = Method::Head;
    pub const POST: Method = Method::Post;
    pub const PUT: Method = Method::Put;
    pub const DELETE: Method = Method::Delete;
    pub const CONNECT: Method = Method::Connect;
    pub const OPTIONS: Method = Method::Options;
    pub const TRACE: Method = Method::Trace;
    pub const PATCH: Method = Method::Patch;
}

impl From<http::Method> for Method {
    fn from(method: http::Method) -> Method {
        match method {
            http::Method::GET => Method::Get,
            http::Method::HEAD => Method::Head,
            http::Method::POST => Method::Post,
            http::Method::PUT => Method::Put,
            http::Method::DELETE => Method::Delete,
            http::Method::CONNECT => Method::Connect,
            http::Method::OPTIONS => Method::Options,
            http::Method::TRACE => Method::Trace,
            http::Method::PATCH => Method::Patch,
            _ => Method::Extension(method.to_string()),
        }
    }
}

impl From<Method> for http::Method {
    fn from(method: Method) -> http::Method {
        match method {
            Method::Get => http::Method::GET,
            Method::Head => http::Method::HEAD,
            Method::Post => http::Method::POST,
            Method::Put => http::Method::PUT,
            Method::Delete => http::Method::DELETE,
            Method::Connect => http::Method::CONNECT,
            Method::Options => http::Method::OPTIONS,
            Method::Trace => http::Method::TRACE,
            Method::Patch => http::Method::PATCH,
            Method::Extension(method) => http::Method::from_bytes(method.as_bytes()).expect("Invalid HTTP method"),
        }
    }
}

/// An HTTP version.
///
/// See associated constants like [`Version::HTTP_11`], or convert from a [`http::Version`].
#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate)]
pub enum Version {
    Http09,
    Http10,
    Http11,
    Http2,
    Http3,
}

impl From<http::Version> for Version {
    fn from(version: http::Version) -> Version {
        match version {
            http::Version::HTTP_09 => Version::Http09,
            http::Version::HTTP_10 => Version::Http10,
            http::Version::HTTP_11 => Version::Http11,
            http::Version::HTTP_2 => Version::Http2,
            http::Version::HTTP_3 => Version::Http3,
            _ => unreachable!("Unknown HTTP version: {version:?}"),
        }
    }
}

impl From<Version> for http::Version {
    fn from(version: Version) -> http::Version {
        match version {
            Version::Http09 => http::Version::HTTP_09,
            Version::Http10 => http::Version::HTTP_10,
            Version::Http11 => http::Version::HTTP_11,
            Version::Http2 => http::Version::HTTP_2,
            Version::Http3 => http::Version::HTTP_3,
        }
    }
}

/// A set of HTTP headers.
///
/// Construct this by converting from a [`http::HeaderMap`].
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct Headers {
    // SATS doesn't (and won't) have a multimap type, so just use an array of pairs for the ser/de format.
    entries: Box<[HttpHeaderPair]>,
}

impl From<http::HeaderMap<http::HeaderValue>> for Headers {
    fn from(value: http::HeaderMap<http::HeaderValue>) -> Headers {
        Headers {
            entries: value
                .into_iter()
                .map(|(name, value)| HttpHeaderPair {
                    name: name.map(|name| name.to_string()).unwrap_or_default(),
                    value: value.into(),
                })
                .collect(),
        }
    }
}

impl TryFrom<Headers> for http::HeaderMap {
    type Error = http::Error;
    fn try_from(headers: Headers) -> http::Result<Self> {
        let Headers { entries } = headers;
        let mut new_headers = http::HeaderMap::with_capacity(entries.len() / 2);
        for HttpHeaderPair { name, value } in entries {
            new_headers.insert(http::HeaderName::try_from(name)?, value.try_into()?);
        }
        Ok(new_headers)
    }
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
struct HttpHeaderPair {
    /// A valid HTTP header name, sourced from an already-validated [`http::HeaderName`].
    name: String,
    value: HeaderValue,
}

/// A valid HTTP header value, sourced from an already-validated [`http::HeaderValue`].
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
struct HeaderValue {
    bytes: Box<[u8]>,
    is_sensitive: bool,
}

impl From<http::HeaderValue> for HeaderValue {
    fn from(value: http::HeaderValue) -> HeaderValue {
        HeaderValue {
            is_sensitive: value.is_sensitive(),
            bytes: value.as_bytes().into(),
        }
    }
}

impl TryFrom<HeaderValue> for http::HeaderValue {
    type Error = http::Error;
    fn try_from(value: HeaderValue) -> http::Result<http::HeaderValue> {
        let mut new_value = http::HeaderValue::from_bytes(&value.bytes)?;
        new_value.set_sensitive(value.is_sensitive);
        Ok(new_value)
    }
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct Response {
    inner: HttpResponse,
}

impl TryFrom<Response> for http::response::Parts {
    type Error = http::Error;
    fn try_from(response: Response) -> http::Result<http::response::Parts> {
        let Response {
            inner: HttpResponse { headers, version, code },
        } = response;

        let (mut response, ()) = http::Response::new(()).into_parts();
        response.version = version.into();
        response.status = http::StatusCode::from_u16(code)?;
        response.headers = headers.try_into()?;
        Ok(response)
    }
}

impl From<http::response::Parts> for Response {
    fn from(response: http::response::Parts) -> Response {
        let http::response::Parts {
            extensions,
            headers,
            status,
            version,
            ..
        } = response;
        if !extensions.is_empty() {
            log::warn!("Converting HTTP `Response` with unrecognized extensions");
        }
        Response {
            inner: HttpResponse {
                headers: headers.into(),
                version: version.into(),
                code: status.as_u16(),
            },
        }
    }
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
struct HttpResponse {
    headers: Headers,
    version: Version,
    /// A valid HTTP response status code, sourced from an already-validated [`http::StatusCode`].
    code: u16,
}

/// Errors that may arise from HTTP calls.
#[derive(Clone, SpacetimeType, Debug)]
#[sats(crate = crate)]
pub struct Error {
    /// A string message describing the error.
    ///
    /// It would be nice if we could store a more interesting object here,
    /// ideally a type-erased `dyn Trait` cause,
    /// rather than just a string, similar to how `anyhow` does.
    /// This is not possible because we need to serialize `Error` for transport to WASM,
    /// meaning it must have a concrete static type.
    /// `reqwest::Error`, which is the source for these,
    /// is type-erased enough that the best we can do (at least, the best we can do easily)
    /// is to eagerly string-ify the error.    
    message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Error { message } = self;
        f.write_str(message)
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn from_string(message: String) -> Self {
        Error { message }
    }

    pub fn from_display(t: &impl std::fmt::Display) -> Self {
        Self::from_string(format!("{t}"))
    }
}
