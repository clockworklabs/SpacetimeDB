//! `SpacetimeType`-ified HTTP request, response and error types,
//! for use in the procedure HTTP API.
//!
//! The types here are all mirrors of various types within the [`http`] crate.
//! That crate's types don't have stable representations or `pub`lic interiors,
//! so we're forced to define our own representation for the SATS serialization.
//! These types are that representation.
//!
//! To preserve extensibility and compatibility, all types defined here should be
//! a `pub` wrapper struct with a single private `inner` field,
//! which is an `enum` whose last variant holds either `Box<str>`, `String`, `Box<[T]>` or `Vec<T>` for any type `T`.
//! Using an enum allows us to add additional variants while preserving the BSATN encoding passed across the WASM boundary,
//! and including a variant with a variable-length type
//! allows us to add other variants with variable-length types while preserving the BFLATN layout stored in table pages.
//! (It's unlikely that any of these types will end up stored in a table, but better safe than sorry.)
//!
//! Users aren't intended to interact with these types, except [`Body`], [`Timeout`] and [`Error`].
//! Our user-facing APIs should use the [`http`] crate's types directly, and convert to and from these types internally.

use spacetimedb_sats::{time_duration::TimeDuration, SpacetimeType};

/// Represents an HTTP request which can be made from a procedure running in a SpacetimeDB database.
///
/// Construct instances of this type by converting from [`http::Request`].
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct Request {
    inner: HttpRequest,
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
            inner: HttpRequest {
                method: method.into(),
                headers: headers.into(),
                timeout,
                uri: uri.to_string(),
                version: version.into(),
            },
        }
    }
}

impl TryFrom<Request> for http::request::Parts {
    type Error = http::Error;
    fn try_from(req: Request) -> http::Result<http::request::Parts> {
        let Request {
            inner:
                HttpRequest {
                    method,
                    headers,
                    timeout,
                    uri,
                    version,
                },
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

/// Represents an HTTP request which can be made from a procedure running in a SpacetimeDB database.
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
struct HttpRequest {
    method: Method,
    headers: Headers,
    timeout: Option<Timeout>,
    /// A valid URI, sourced from an already-validated [`http::Uri`].
    uri: String,
    version: Version,
}

/// An HTTP extension to specify a timeout for requests made by a procedure running in a SpacetimeDB database.
///
/// Pass an instance of this type to [`http::request::Builder::extension`] to set a timeout on a request.
// This type is a user-facing trivial newtype, no need for all the struct-wrapping-enum compatibility song-and-dance.
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
///
/// See associated constants like [`Method::GET`], or convert from a [`http::Method`].
#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate)]
pub struct Method {
    inner: HttpMethod,
}

#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate)]
enum HttpMethod {
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
    const fn from_inner(inner: HttpMethod) -> Method {
        Method { inner }
    }

    pub const GET: Method = Method::from_inner(HttpMethod::Get);
    pub const HEAD: Method = Method::from_inner(HttpMethod::Head);
    pub const POST: Method = Method::from_inner(HttpMethod::Post);
    pub const PUT: Method = Method::from_inner(HttpMethod::Put);
    pub const DELETE: Method = Method::from_inner(HttpMethod::Delete);
    pub const CONNECT: Method = Method::from_inner(HttpMethod::Connect);
    pub const OPTIONS: Method = Method::from_inner(HttpMethod::Options);
    pub const TRACE: Method = Method::from_inner(HttpMethod::Trace);
    pub const PATCH: Method = Method::from_inner(HttpMethod::Patch);
}

impl From<http::Method> for Method {
    fn from(method: http::Method) -> Method {
        match method {
            http::Method::GET => Method::GET,
            http::Method::HEAD => Method::HEAD,
            http::Method::POST => Method::POST,
            http::Method::PUT => Method::PUT,
            http::Method::DELETE => Method::DELETE,
            http::Method::CONNECT => Method::CONNECT,
            http::Method::OPTIONS => Method::OPTIONS,
            http::Method::TRACE => Method::TRACE,
            http::Method::PATCH => Method::PATCH,
            _ => Method {
                inner: HttpMethod::Extension(method.to_string()),
            },
        }
    }
}

impl From<Method> for http::Method {
    fn from(method: Method) -> http::Method {
        match method {
            Method::GET => http::Method::GET,
            Method::HEAD => http::Method::HEAD,
            Method::POST => http::Method::POST,
            Method::PUT => http::Method::PUT,
            Method::DELETE => http::Method::DELETE,
            Method::CONNECT => http::Method::CONNECT,
            Method::OPTIONS => http::Method::OPTIONS,
            Method::TRACE => http::Method::TRACE,
            Method::PATCH => http::Method::PATCH,
            Method {
                inner: HttpMethod::Extension(method),
            } => http::Method::from_bytes(method.as_bytes()).expect("Invalid HTTP method"),
        }
    }
}

/// An HTTP version.
///
/// See associated constants like [`Version::HTTP_11`], or convert from a [`http::Version`].
#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate)]
pub struct Version {
    inner: HttpVersion,
}

impl Version {
    const fn from_inner(inner: HttpVersion) -> Version {
        Version { inner }
    }

    pub const HTTP_09: Version = Version::from_inner(HttpVersion::Http09);
    pub const HTTP_10: Version = Version::from_inner(HttpVersion::Http10);
    pub const HTTP_11: Version = Version::from_inner(HttpVersion::Http11);
    pub const HTTP_2: Version = Version::from_inner(HttpVersion::Http2);
    pub const HTTP_3: Version = Version::from_inner(HttpVersion::Http3);
}

impl From<http::Version> for Version {
    fn from(version: http::Version) -> Version {
        match version {
            http::Version::HTTP_09 => Version::HTTP_09,
            http::Version::HTTP_10 => Version::HTTP_10,
            http::Version::HTTP_11 => Version::HTTP_11,
            http::Version::HTTP_2 => Version::HTTP_2,
            http::Version::HTTP_3 => Version::HTTP_3,
            _ => unreachable!("Unknown HTTP version: {version:?}"),
        }
    }
}

impl From<Version> for http::Version {
    fn from(version: Version) -> http::Version {
        match version {
            Version::HTTP_09 => http::Version::HTTP_09,
            Version::HTTP_10 => http::Version::HTTP_10,
            Version::HTTP_11 => http::Version::HTTP_11,
            Version::HTTP_2 => http::Version::HTTP_2,
            Version::HTTP_3 => http::Version::HTTP_3,
        }
    }
}

#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate)]
enum HttpVersion {
    Http09,
    Http10,
    Http11,
    Http2,
    Http3,
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

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct Error {
    inner: HttpError,
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Error {
            inner: HttpError::Message(msg),
        } = self;
        f.debug_tuple("spacetimedb_lib::http::Error").field(msg).finish()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Error {
            inner: HttpError::Message(msg),
        } = self;
        f.write_str(msg)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate)]
enum HttpError {
    // It would be nice if we could store a more interesting object here,
    // ideally a type-erased `dyn Trait` cause,
    // rather than just a string, similar to how `anyhow` does.
    // This is not possible because we need to serialize `Error` for transport to WASM,
    // meaning it must have a concrete static type.
    // `reqwest::Error`, which is the source for these,
    // is type-erased enough that the best we can do (at least, the best we can do easily)
    // is to eagerly string-ify the error.
    Message(String),
}

impl Error {
    pub fn from_string(message: String) -> Self {
        Error {
            inner: HttpError::Message(message),
        }
    }

    pub fn from_display(t: &impl std::fmt::Display) -> Self {
        Self::from_string(format!("{t}"))
    }
}
