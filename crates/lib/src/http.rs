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
struct HeaderIter<I, T> {
    prev: Option<(String, T)>,
    inner: I,
}

impl<I, T> Iterator for HeaderIter<I, T>
where
    I: Iterator<Item = (Option<String>, T)>,
{
    type Item = (String, T);

    fn next(&mut self) -> Option<Self::Item> {
        let (prev_k, prev_v) = self
            .prev
            .take()
            .or_else(|| self.inner.next().map(|(k, v)| (k.unwrap(), v)))?;
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

impl FromIterator<(Option<String>, HeaderValue)> for Headers {
    fn from_iter<T: IntoIterator<Item = (Option<String>, HeaderValue)>>(iter: T) -> Self {
        let inner = iter.into_iter();
        let entries = HeaderIter { prev: None, inner }
            .map(|(name, value)| HttpHeaderPair { name, value })
            .collect();
        Self { entries }
    }
}

impl Headers {
    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> impl Iterator<Item = (String, HeaderValue)> {
        IntoIterator::into_iter(self.entries).map(|HttpHeaderPair { name, value }| (name, value))
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
pub struct HeaderValue {
    pub bytes: Box<[u8]>,
    pub is_sensitive: bool,
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpResponse")]
pub struct Response {
    pub headers: Headers,
    pub version: Version,
    /// A valid HTTP response status code, sourced from an already-validated [`http::StatusCode`].
    pub code: u16,
}
