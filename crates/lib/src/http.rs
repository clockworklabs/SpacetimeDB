//! `SpacetimeType`-ified HTTP request and response types,
//! for use in the procedure HTTP API.
//!
//! The types here are all mirrors of various types within the `http` crate.
//! That crate's types don't have stable representations or `pub`lic interiors,
//! so we're forced to define our own representation for the SATS serialization.
//! These types are that representation.
//!
//! Users aren't intended to interact with these types,
//! Our user-facing APIs should use the `http` crate's types directly, and convert to and from these types internally.
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
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpRequest")]
pub struct Request {
    pub method: Method,
    pub headers: Headers,
    pub timeout: Option<TimeDuration>,
    /// A valid URI, sourced from an already-validated `http::Uri`.
    pub uri: String,
    pub version: Version,
}

impl Request {
    /// Return the size of this request's URI and [`Headers`]
    /// for purposes of metrics reporting.
    ///
    /// Ignores the size of the [`Method`] and [`Version`] as they are effectively constant.
    ///
    /// As the body is stored externally to the `Request`, metrics reporting must count its size separately.
    pub fn size_in_bytes(&self) -> usize {
        self.uri.len() + self.headers.size_in_bytes()
    }
}

/// Represents an HTTP method.
#[derive(Clone, Debug, SpacetimeType, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpHeaders")]
pub struct Headers {
    // SATS doesn't (and won't) have a multimap type, so just use an array of pairs for the ser/de format.
    entries: Box<[HttpHeaderPair]>,
}

// `http::header::IntoIter` only returns the `HeaderName` for the first
// `HeaderValue` with that name, so we have to manually assign the names.
struct HeaderIter<I, T> {
    prev: Option<(Box<str>, T)>,
    inner: I,
}

impl<I, T> Iterator for HeaderIter<I, T>
where
    I: Iterator<Item = (Option<Box<str>>, T)>,
{
    type Item = (Box<str>, T);

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

impl FromIterator<(Option<Box<str>>, Box<[u8]>)> for Headers {
    fn from_iter<T: IntoIterator<Item = (Option<Box<str>>, Box<[u8]>)>>(iter: T) -> Self {
        let inner = iter.into_iter();
        let entries = HeaderIter { prev: None, inner }
            .map(|(name, value)| HttpHeaderPair { name, value })
            .collect();
        Self { entries }
    }
}

impl Headers {
    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> impl Iterator<Item = (Box<str>, Box<[u8]>)> {
        IntoIterator::into_iter(self.entries).map(|HttpHeaderPair { name, value }| (name, value))
    }

    /// The sum of the lengths of all the header names and header values.
    ///
    /// For headers with multiple values for the same header name,
    /// the length of the header name is counted once for each occurrence.
    fn size_in_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|HttpHeaderPair { name, value }| name.len() + value.len())
            .sum::<usize>()
    }
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpHeaderPair")]
struct HttpHeaderPair {
    /// A valid HTTP header name, sourced from an already-validated `http::HeaderName`.
    name: Box<str>,
    /// A valid HTTP header value, sourced from an already-validated `http::HeaderValue`.
    value: Box<[u8]>,
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = crate, name = "HttpResponse")]
pub struct Response {
    pub headers: Headers,
    pub version: Version,
    /// A valid HTTP response status code, sourced from an already-validated `http::StatusCode`.
    pub code: u16,
}

impl Response {
    /// Return the size of this request's [`Headers`] for purposes of metrics reporting.
    ///
    /// Ignores the size of the `code` and [`Version`] as they are effectively constant.
    ///
    /// As the body is stored externally to the `Response`, metrics reporting must count its size separately.
    pub fn size_in_bytes(&self) -> usize {
        self.headers.size_in_bytes()
    }
}

/// True if `c` is a valid character to appear in the path of a user-defined HTTP route.
///
/// We permit only lowercase ASCII letters, ASCII digits, and `-_~/*`.
/// `/` is allowed specifically because it's the segment separator character.
/// `-_~` seem harmless enough.
/// `*` is the wildcard character; it is valid only as the final character of a path,
/// where it matches any remaining path. [`wildcard_is_only_trailing`] checks that position.
///
/// We've chosen an intentionally very restrictive set so that we can assign meaning to other characters in the future,
/// e.g. we may want to use `:` or `{}` to introduce path parameters.
pub fn character_is_acceptable_for_route_path(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || "-_~/*".contains(c)
}

/// A human-readable description of the characters accepted by [`character_is_acceptable_for_route_path`],
/// for use in error reporting.
pub const ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION: &str = "ASCII lowercase letters, digits and `-_~/*`";

/// The character which, as the final character of a route path, matches any remaining path.
pub const ROUTE_WILDCARD: char = '*';

/// True if `*` appears at most once in `path`, and only as its final character.
pub fn wildcard_is_only_trailing(path: &str) -> bool {
    match path.strip_suffix(ROUTE_WILDCARD) {
        Some(prefix) => !prefix.contains(ROUTE_WILDCARD),
        None => !path.contains(ROUTE_WILDCARD),
    }
}

/// True if the route path `route_path` matches the request path `request_path`.
///
/// A wildcard route matches any request path starting with the prefix before its `*`,
/// so `/webhooks/*` matches `/webhooks/github` and `/webhooks/` but not `/webhooks`.
/// A static route matches only itself.
pub fn route_path_matches(route_path: &str, request_path: &str) -> bool {
    match route_path.strip_suffix(ROUTE_WILDCARD) {
        Some(prefix) => request_path.starts_with(prefix),
        None => route_path == request_path,
    }
}

/// True if two route paths can match the same request path.
///
/// Static paths overlap only when they are equal; a wildcard path overlaps any path
/// which starts with the prefix before its `*`, including another wildcard.
pub fn route_paths_overlap(a: &str, b: &str) -> bool {
    match (a.strip_suffix(ROUTE_WILDCARD), b.strip_suffix(ROUTE_WILDCARD)) {
        (Some(a_prefix), Some(b_prefix)) => a_prefix.starts_with(b_prefix) || b_prefix.starts_with(a_prefix),
        (Some(prefix), None) => b.starts_with(prefix),
        (None, Some(prefix)) => a.starts_with(prefix),
        (None, None) => a == b,
    }
}

#[cfg(test)]
mod route_path_tests {
    use super::*;

    #[test]
    fn wildcard_must_be_trailing() {
        assert!(wildcard_is_only_trailing("/webhooks/*"));
        assert!(wildcard_is_only_trailing("/webhooks"));
        assert!(wildcard_is_only_trailing(""));

        // A lone `*` is correctly placed; it is rejected by the leading-slash rule, not here.
        assert!(wildcard_is_only_trailing("*"));
        assert!(!wildcard_is_only_trailing("/mid*path"));
        assert!(!wildcard_is_only_trailing("/double**"));
        assert!(!wildcard_is_only_trailing("/a/*/b"));
    }

    #[test]
    fn static_route_matches_only_itself() {
        assert!(route_path_matches("/webhooks/github", "/webhooks/github"));
        assert!(!route_path_matches("/webhooks/github", "/webhooks/github/"));
        assert!(!route_path_matches("/webhooks", "/webhooks/github"));
    }

    #[test]
    fn wildcard_route_matches_the_remainder() {
        assert!(route_path_matches("/webhooks/*", "/webhooks/github"));
        assert!(route_path_matches("/webhooks/*", "/webhooks/"));
        assert!(route_path_matches("/webhooks/*", "/webhooks/a/b"));

        // The prefix itself, without its trailing slash, is not matched.
        assert!(!route_path_matches("/webhooks/*", "/webhooks"));
        assert!(!route_path_matches("/webhooks/*", "/other/github"));
    }

    #[test]
    fn static_paths_overlap_only_when_equal() {
        assert!(route_paths_overlap("/a", "/a"));
        assert!(!route_paths_overlap("/a", "/b"));
        assert!(!route_paths_overlap("/a", "/a/b"));
    }

    #[test]
    fn wildcard_overlaps_its_prefix_and_everything_below() {
        assert!(route_paths_overlap("/a/*", "/a/b"));
        assert!(route_paths_overlap("/a/b", "/a/*"));
        assert!(!route_paths_overlap("/a/*", "/b/c"));
        assert!(!route_paths_overlap("/a/*", "/a"));
    }

    #[test]
    fn wildcards_overlap_when_either_prefix_contains_the_other() {
        assert!(route_paths_overlap("/a/*", "/a/b/*"));
        assert!(route_paths_overlap("/a/*", "/a/*"));
        assert!(!route_paths_overlap("/a/*", "/b/*"));
    }
}
