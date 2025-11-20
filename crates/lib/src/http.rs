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
/// Note that all extensions to [`http::Request`] save for [`Timeout`] are ignored.
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpRequest")]
pub struct Request {
    pub method: Method,
    pub headers: Headers,
    pub timeout: Option<TimeDuration>,
    /// A valid URI, sourced from an already-validated [`http::Uri`].
    pub uri: String,
    pub version: Version,
}

/// Represents an HTTP method.
#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate, name = "HttpMethod")]
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
#[derive(Clone, SpacetimeType, PartialEq, Eq)]
#[sats(crate = crate, name = "HttpVersion")]
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
#[sats(crate = crate, name = "HttpHeaders")]
pub struct Headers {
    // SATS doesn't (and won't) have a multimap type, so just use an array of pairs for the ser/de format.
    entries: Box<[HttpHeaderPair]>,
}

// `http::header::IntoIter` only returns the `HeaderName` for the first
// `HeaderValue` with that name, so we have to manually assign the names.
struct HeaderMapIntoIter {
    prev: Option<(http::HeaderName, http::HeaderValue)>,
    inner: http::header::IntoIter<http::HeaderValue>,
}

impl From<http::header::HeaderMap> for HeaderMapIntoIter {
    fn from(map: http::header::HeaderMap) -> Self {
        let mut inner = map.into_iter();
        Self {
            prev: inner.next().map(|(k, v)| (k.unwrap(), v)),
            inner,
        }
    }
}

impl Iterator for HeaderMapIntoIter {
    type Item = (http::HeaderName, http::HeaderValue);

    fn next(&mut self) -> Option<Self::Item> {
        let (prev_k, prev_v) = self.prev.take()?;
        self.prev = self
            .inner
            .next()
            .map(|(next_k, next_v)| (next_k.unwrap_or_else(|| prev_k.clone()), next_v));
        Some((prev_k, prev_v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl From<http::HeaderMap> for Headers {
    fn from(value: http::HeaderMap) -> Headers {
        Headers {
            entries: HeaderMapIntoIter::from(value)
                .map(|(name, value)| HttpHeaderPair {
                    name: name.to_string(),
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
#[sats(crate = crate, name = "HttpHeaderPair")]
struct HttpHeaderPair {
    /// A valid HTTP header name, sourced from an already-validated [`http::HeaderName`].
    name: String,
    value: HeaderValue,
}

/// A valid HTTP header value, sourced from an already-validated [`http::HeaderValue`].
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpHeaderValue")]
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
#[sats(crate = crate, name = "HttpResponse")]
pub struct Response {
    pub headers: Headers,
    pub version: Version,
    /// A valid HTTP response status code, sourced from an already-validated [`http::StatusCode`].
    pub code: u16,
}
