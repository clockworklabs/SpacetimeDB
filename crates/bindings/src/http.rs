//! Types and utilities for performing HTTP requests in [procedures](crate::procedure).
//!
//! Perform an HTTP request using methods on [`crate::ProcedureContext::http`],
//! which is of type [`HttpClient`].
//! The [`get`](HttpClient::get) helper can be used for simple `GET` requests,
//! while [`send`](HttpClient::send) allows more complex requests with headers, bodies and other methods.

use crate::{
    rt::{read_bytes_source_as, read_bytes_source_into},
    IterBuf, ReducerContext, StdbRng, Timestamp, TxContext,
};
use bytes::Bytes;
#[cfg(feature = "rand")]
use rand08::RngCore;
use spacetimedb_lib::db::raw_def::v10::MethodOrAny;
use spacetimedb_lib::http::{
    self as st_http, character_is_acceptable_for_route_path, ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION,
};
use spacetimedb_lib::{bsatn, Identity, TimeDuration, Uuid};
use std::cell::{Cell, OnceCell};
use std::str::FromStr;

pub type Request<T = Body> = http::Request<T>;

pub type Response<T = Body> = http::Response<T>;

pub use spacetimedb_bindings_macro::{http_handler as handler, http_router as router};

/// The context that any HTTP handler is provided with.
///
/// Each HTTP handler must accept `&mut spacetimedb::http::HandlerContext` as its first argument.
///
/// Includes the time of invocation and exposes methods for running transactions
/// and performing side-effecting operations.
#[non_exhaustive]
pub struct HandlerContext {
    /// The time at which the handler was started.
    pub timestamp: Timestamp,

    /// Methods for performing HTTP requests.
    pub http: HttpClient,

    #[cfg(feature = "rand08")]
    pub(crate) rng: OnceCell<StdbRng>,

    /// A counter used for generating UUIDv7 values.
    /// **Note:** must be 0..=u32::MAX
    #[cfg(feature = "rand")]
    pub(crate) counter_uuid: Cell<u32>,
}

impl HandlerContext {
    pub(crate) fn new(timestamp: Timestamp) -> Self {
        Self {
            timestamp,
            http: HttpClient {},
            #[cfg(feature = "rand08")]
            rng: OnceCell::new(),
            #[cfg(feature = "rand")]
            counter_uuid: Cell::new(0),
        }
    }

    /// Read the current module's [`Identity`].
    pub fn identity(&self) -> Identity {
        Identity::from_byte_array(spacetimedb_bindings_sys::identity())
    }

    /// Acquire a mutable transaction and execute `body` with read-write access.
    pub fn with_tx<T>(&mut self, body: impl Fn(&TxContext) -> T) -> T {
        use core::convert::Infallible;
        match self.try_with_tx::<T, Infallible>(|tx| Ok(body(tx))) {
            Ok(v) => v,
            Err(e) => match e {},
        }
    }

    /// Acquire a mutable transaction and execute `body` with read-write access.
    pub fn try_with_tx<T, E>(&mut self, body: impl Fn(&TxContext) -> Result<T, E>) -> Result<T, E> {
        let abort = || {
            crate::sys::procedure::procedure_abort_mut_tx()
                .expect("should have a pending mutable anon tx as `procedure_start_mut_tx` preceded")
        };

        let run = || {
            let timestamp = crate::sys::procedure::procedure_start_mut_tx()
                .expect("holding `&mut HandlerContext`, so should not be in a tx already; called manually elsewhere?");
            let timestamp = Timestamp::from_micros_since_unix_epoch(timestamp);

            // Use the internal auth context (no external caller identity).
            let tx = ReducerContext::new(crate::Local {}, Identity::ZERO, None, timestamp);
            let tx = TxContext(tx);

            struct DoOnDrop<F: Fn()>(F);
            impl<F: Fn()> Drop for DoOnDrop<F> {
                fn drop(&mut self) {
                    (self.0)();
                }
            }
            let abort_guard = DoOnDrop(abort);
            let res = body(&tx);
            core::mem::forget(abort_guard);
            res
        };

        let mut res = run();

        match res {
            Ok(_) if crate::sys::procedure::procedure_commit_mut_tx().is_err() => {
                log::warn!("committing anonymous transaction failed");
                res = run();
                match res {
                    Ok(_) => crate::sys::procedure::procedure_commit_mut_tx().expect("transaction retry failed again"),
                    Err(_) => abort(),
                }
            }
            Ok(_) => {}
            Err(_) => abort(),
        }

        res
    }

    /// Create a new random [`Uuid`] `v4` using the built-in RNG.
    #[cfg(feature = "rand")]
    pub fn new_uuid_v4(&self) -> anyhow::Result<Uuid> {
        let mut bytes = [0u8; 16];
        self.rng().try_fill_bytes(&mut bytes)?;
        Ok(Uuid::from_random_bytes_v4(bytes))
    }

    /// Create a new sortable [`Uuid`] `v7` using the built-in RNG, counter and timestamp.
    #[cfg(feature = "rand")]
    pub fn new_uuid_v7(&self) -> anyhow::Result<Uuid> {
        let mut random_bytes = [0u8; 4];
        self.rng().try_fill_bytes(&mut random_bytes)?;
        Uuid::from_counter_v7(&self.counter_uuid, self.timestamp, &random_bytes)
    }
}

/// Describes an HTTP handler function for use with [`Router`].
///
/// The [`handler`] macro will define a constant of type [`Handler`],
/// which can be used to refer to the handler function when registering it to handle a route.
#[derive(Clone, Copy)]
pub struct Handler {
    name: &'static str,
}

impl Handler {
    /// Emitted by the [`handler`] macro.
    ///
    /// User code should not call this method. In order for a `Handler` to be valid,
    /// its `name` must refer to a function registered with the SpacetimeDB host as an HTTP handler.
    /// The only supported way to do this is by annotating a function with the [`handler`] macro.
    #[doc(hidden)]
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }
}

#[derive(Clone, Default)]
pub struct Router {
    routes: Vec<RouteSpec>,
}

#[derive(Clone)]
pub(crate) struct RouteSpec {
    pub method: MethodOrAny,
    pub path: String,
    pub handler: Handler,
}

impl Router {
    /// Returns a new, empty `Router`.
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Registers `handler` to handle `GET` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn get(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Get), path, handler)
    }

    /// Registers `handler` to handle `HEAD` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn head(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Head), path, handler)
    }

    /// Registers `handler` to handle `OPTIONS` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn options(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Options), path, handler)
    }

    /// Registers `handler` to handle `PUT` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn put(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Put), path, handler)
    }

    /// Registers `handler` to handle `DELETE` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn delete(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Delete), path, handler)
    }

    /// Registers `handler` to handle `POST` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn post(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Post), path, handler)
    }

    /// Registers `handler` to handle `PATCH` requests at `path`.
    ///
    /// Panics if `self` already has a handler on this method at this path,
    /// including one registered with [`Self::any`],
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn patch(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Method(st_http::Method::Patch), path, handler)
    }

    /// Registers `handler` to handle requests of any HTTP method at `path`.
    ///
    /// Panics if `self` already has a handler on at least one method at this path,
    /// or if this path overlaps with a nested router registered by [`Self::nest`].
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn any(self, path: impl Into<String>, handler: Handler) -> Self {
        self.add_route(MethodOrAny::Any, path, handler)
    }

    /// Causes requests which start with `path` to be processed by `sub_router`.
    ///
    /// `sub_router` will be used by stripping the leading `path` from the path of the request.
    ///
    /// Panics if `self` already has any handlers registered on paths which start with `path`.
    ///
    /// Panics if the `path` does not begin with `/`, or if it contains any characters which are not URL-safe.
    pub fn nest(self, path: impl Into<String>, sub_router: Self) -> Self {
        let path = path.into();
        assert_valid_path(&path);

        // FIXME: either this check is too restrictive, or the checks in the other methods are too lenient.
        // Do we want it to be the case that the `sub_router` effectively takes ownership of the whole route below `path`,
        // or just the routes it actually contains?
        if self.routes.iter().any(|route| route.path.starts_with(&path)) {
            panic!("Cannot nest router at `{path}`; existing routes overlap with nested path");
        }

        let mut merged = self;
        for route in sub_router.routes {
            let nested_path = join_paths(&path, &route.path);
            merged = merged.add_route(route.method, nested_path, route.handler);
        }
        merged
    }

    /// Combines all of the routes in `self` and `other_router` into a single [`Router`].
    ///
    /// Panics if any of the routes in `self` conflict with any of the routes in `other_router`.
    pub fn merge(self, other_router: Self) -> Self {
        let mut merged = self;
        for route in other_router.routes {
            merged = merged.add_route(route.method, route.path, route.handler);
        }
        merged
    }

    pub(crate) fn into_routes(self) -> Vec<RouteSpec> {
        self.routes
    }

    fn add_route(mut self, method: MethodOrAny, path: impl Into<String>, handler: Handler) -> Self {
        let path = path.into();
        assert_valid_path(&path);

        let candidate = RouteSpec {
            method: method.clone(),
            path: path.clone(),
            handler,
        };

        // TODO(perf): Adding a route is O(n), which means that building a router is O(n^2)
        if self.routes.iter().any(|route| routes_overlap(route, &candidate)) {
            panic!("Route conflict for `{path}`");
        }

        self.routes.push(candidate);
        self
    }
}

fn join_paths(prefix: &str, suffix: &str) -> String {
    if prefix == "/" {
        return suffix.to_string();
    }
    if suffix == "/" {
        return prefix.to_string();
    }
    let prefix = prefix.trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');
    format!("{prefix}/{suffix}")
}

fn assert_valid_path(path: &str) {
    if !path.is_empty() && !path.starts_with('/') {
        panic!("Route paths must start with `/`: {path}");
    }
    if !path.chars().all(character_is_acceptable_for_route_path) {
        panic!(
            "Route paths may contain only {}: {path}",
            ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION
        );
    }
}

fn routes_overlap(a: &RouteSpec, b: &RouteSpec) -> bool {
    if a.path != b.path {
        return false;
    }
    matches!(a.method, MethodOrAny::Any) || matches!(b.method, MethodOrAny::Any) || a.method == b.method
}

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
    /// ```no_run
    /// # use spacetimedb::{procedure, ProcedureContext, TimeDuration, http::{Timeout, Request}};
    /// # use std::time::Duration;
    /// # #[procedure]
    /// # fn post_somewhere(ctx: &mut ProcedureContext) {
    /// let request = Request::builder()
    ///     .uri("https://some-remote-host.invalid/upload")
    ///     .method("POST")
    ///     .header("Content-Type", "text/plain")
    ///     // Set a timeout of 100 ms, further restricting the default timeout.
    ///     .extension(Timeout::from(TimeDuration::from(Duration::from_millis(100))))
    ///     .body("This is the body of the HTTP request")
    ///     .expect("Building `Request` object failed");
    ///
    /// match ctx.http.send(request) {
    ///     Err(err) => {
    ///         log::error!("HTTP request failed: {err}");
    ///     },
    ///     Ok(response) => {
    ///         let (parts, body) = response.into_parts();
    ///         log::info!(
    ///             "Got response with status {}, body {}",
    ///             parts.status,
    ///             body.into_string_lossy(),
    ///         );
    ///     }
    /// }
    /// # }
    ///
    /// ```
    pub fn send<B: Into<Body>>(&self, request: http::Request<B>) -> Result<Response, Error> {
        let (request, body) = request.map(Into::into).into_parts();
        let request = convert_request(request);
        let request = bsatn::to_vec(&request).expect("Failed to BSATN-serialize `spacetimedb_lib::http::Request`");

        match spacetimedb_bindings_sys::procedure::http_request(&request, &body.into_bytes()) {
            Ok((response_source, body_source)) => {
                let response = read_bytes_source_as::<st_http::Response>(response_source);
                let response = convert_response(response).expect("Invalid http response returned from host");
                let body = if body_source == spacetimedb_bindings_sys::raw::BytesSource::INVALID {
                    // Empty response body — host returns INVALID source for empty bytes
                    Body::from_bytes(Vec::<u8>::new())
                } else {
                    let mut buf = IterBuf::take();
                    read_bytes_source_into(body_source, &mut buf);
                    Body::from_bytes(buf.clone())
                };

                Ok(http::Response::from_parts(response, body))
            }
            Err(err_source) => {
                let message = read_bytes_source_as::<String>(err_source);
                Err(Error { message })
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
    /// # use spacetimedb::{procedure, ProcedureContext};
    /// # #[procedure]
    /// # fn get_from_somewhere(ctx: &mut ProcedureContext) {
    /// match ctx.http.get("https://some-remote-host.invalid/download") {
    ///     Err(err) => {
    ///         log::error!("HTTP request failed: {err}");
    ///     }
    ///     Ok(response) => {
    ///         let (parts, body) = response.into_parts();
    ///         log::info!(
    ///             "Got response with status {}, body {}",
    ///             parts.status,
    ///             body.into_string_lossy(),
    ///         );
    ///     }
    /// }
    /// # }
    /// ```
    pub fn get(&self, uri: impl TryInto<http::Uri, Error: Into<http::Error>>) -> Result<Response, Error> {
        self.send(
            http::Request::builder()
                .method(http::Method::GET)
                .uri(uri)
                .body(Body::empty())?,
        )
    }
}

fn convert_request(parts: http::request::Parts) -> st_http::Request {
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
    st_http::Request {
        method: match method {
            http::Method::GET => st_http::Method::Get,
            http::Method::HEAD => st_http::Method::Head,
            http::Method::POST => st_http::Method::Post,
            http::Method::PUT => st_http::Method::Put,
            http::Method::DELETE => st_http::Method::Delete,
            http::Method::CONNECT => st_http::Method::Connect,
            http::Method::OPTIONS => st_http::Method::Options,
            http::Method::TRACE => st_http::Method::Trace,
            http::Method::PATCH => st_http::Method::Patch,
            _ => st_http::Method::Extension(method.to_string()),
        },
        headers: headers
            .into_iter()
            .map(|(k, v)| (k.map(|k| k.as_str().into()), v.as_bytes().into()))
            .collect(),
        timeout: timeout.map(Into::into),
        uri: uri.to_string(),
        version: match version {
            http::Version::HTTP_09 => st_http::Version::Http09,
            http::Version::HTTP_10 => st_http::Version::Http10,
            http::Version::HTTP_11 => st_http::Version::Http11,
            http::Version::HTTP_2 => st_http::Version::Http2,
            http::Version::HTTP_3 => st_http::Version::Http3,
            _ => unreachable!("Unknown HTTP version: {version:?}"),
        },
    }
}

fn convert_response(response: st_http::Response) -> http::Result<http::response::Parts> {
    let st_http::Response { headers, version, code } = response;

    let (mut response, ()) = http::Response::new(()).into_parts();
    response.version = match version {
        st_http::Version::Http09 => http::Version::HTTP_09,
        st_http::Version::Http10 => http::Version::HTTP_10,
        st_http::Version::Http11 => http::Version::HTTP_11,
        st_http::Version::Http2 => http::Version::HTTP_2,
        st_http::Version::Http3 => http::Version::HTTP_3,
    };
    response.status = http::StatusCode::from_u16(code)?;
    response.headers = headers
        .into_iter()
        .map(|(k, v)| Ok((k.into_string().try_into()?, v.into_vec().try_into()?)))
        .collect::<http::Result<_>>()?;
    Ok(response)
}

pub(crate) fn request_from_wire(request: st_http::Request, body: Bytes) -> http::Request<Body> {
    let st_http::Request {
        method,
        headers,
        timeout: _,
        uri,
        version,
    } = request;

    let method = match method {
        st_http::Method::Get => http::Method::GET,
        st_http::Method::Head => http::Method::HEAD,
        st_http::Method::Post => http::Method::POST,
        st_http::Method::Put => http::Method::PUT,
        st_http::Method::Delete => http::Method::DELETE,
        st_http::Method::Connect => http::Method::CONNECT,
        st_http::Method::Options => http::Method::OPTIONS,
        st_http::Method::Trace => http::Method::TRACE,
        st_http::Method::Patch => http::Method::PATCH,
        st_http::Method::Extension(ext) => {
            http::Method::from_bytes(ext.as_bytes()).expect("Invalid HTTP method from host")
        }
    };

    let request = http::Request::builder()
        .method(method)
        .uri(http::Uri::from_str(&uri).expect("Invalid URI from host"))
        .body(Body::from_bytes(body))
        .expect("Failed to build request");

    let (mut parts, body) = request.into_parts();
    parts.version = match version {
        st_http::Version::Http09 => http::Version::HTTP_09,
        st_http::Version::Http10 => http::Version::HTTP_10,
        st_http::Version::Http11 => http::Version::HTTP_11,
        st_http::Version::Http2 => http::Version::HTTP_2,
        st_http::Version::Http3 => http::Version::HTTP_3,
    };
    parts.headers = headers
        .into_iter()
        .map(|(k, v)| {
            let name = http::HeaderName::from_bytes(k.as_bytes()).expect("Invalid header name from host");
            let value = http::HeaderValue::from_bytes(v.as_ref()).expect("Invalid header value from host");
            (name, value)
        })
        .collect();

    http::Request::from_parts(parts, body)
}

pub(crate) fn response_into_wire(response: http::Response<Body>) -> (st_http::Response, Bytes) {
    let (parts, body) = response.into_parts();
    let st_response = st_http::Response {
        headers: parts
            .headers
            .into_iter()
            .map(|(k, v)| (k.map(|k| k.as_str().into()), v.as_bytes().into()))
            .collect(),
        version: match parts.version {
            http::Version::HTTP_09 => st_http::Version::Http09,
            http::Version::HTTP_10 => st_http::Version::Http10,
            http::Version::HTTP_11 => st_http::Version::Http11,
            http::Version::HTTP_2 => st_http::Version::Http2,
            http::Version::HTTP_3 => st_http::Version::Http3,
            _ => unreachable!("Unknown HTTP version: {:?}", parts.version),
        },
        code: parts.status.as_u16(),
    };

    // TODO(streaming-http): stop collecting the whole response body here once handler
    // responses can write incrementally to a body sink.
    (st_response, body.into_bytes())
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

/// An HTTP extension to specify a timeout for requests made by a procedure running in a SpacetimeDB database.
///
/// Pass an instance of this type to [`http::request::Builder::extension`] to set a timeout on a request.
///
/// This timeout applies to the entire request,
/// from when the headers are first sent to when the response body is fully downloaded.
/// This is sometimes called a total timeout, the sum of the connect timeout and the read timeout.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Timeout(pub TimeDuration);

impl From<TimeDuration> for Timeout {
    fn from(timeout: TimeDuration) -> Timeout {
        Timeout(timeout)
    }
}

impl From<Timeout> for TimeDuration {
    fn from(Timeout(timeout): Timeout) -> TimeDuration {
        timeout
    }
}

/// An error that may arise from an HTTP call.
#[derive(Clone, Debug)]
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

impl From<http::Error> for Error {
    fn from(err: http::Error) -> Self {
        Error {
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_from_wire_preserves_metadata_and_body() {
        let request = st_http::Request {
            method: st_http::Method::Post,
            headers: vec![
                (
                    Some("content-type".into()),
                    b"application/octet-stream".as_slice().into(),
                ),
                (Some("x-echo".into()), b"value".as_slice().into()),
            ]
            .into_iter()
            .collect(),
            timeout: None,
            uri: "https://example.invalid/upload?x=1".to_string(),
            version: st_http::Version::Http2,
        };

        let request = request_from_wire(request, Bytes::from_static(b"payload"));

        assert_eq!(request.method(), http::Method::POST);
        assert_eq!(request.version(), http::Version::HTTP_2);
        assert_eq!(
            request.uri(),
            &http::Uri::from_static("https://example.invalid/upload?x=1")
        );
        assert_eq!(request.headers()["content-type"], "application/octet-stream");
        assert_eq!(request.headers()["x-echo"], "value");
        assert_eq!(request.into_body().into_bytes(), Bytes::from_static(b"payload"));
    }

    #[test]
    fn response_into_wire_splits_metadata_and_body() {
        let response = http::Response::builder()
            .status(201)
            .version(http::Version::HTTP_11)
            .header("content-type", "text/plain")
            .header("x-result", "ok")
            .body(Body::from_bytes("created"))
            .expect("response builder should not fail");

        let (response_meta, response_body) = response_into_wire(response);

        assert_eq!(response_meta.code, 201);
        assert!(matches!(response_meta.version, st_http::Version::Http11));

        let headers = response_meta.headers.into_iter().collect::<Vec<_>>();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0.as_ref(), "content-type");
        assert_eq!(&headers[0].1[..], b"text/plain");
        assert_eq!(headers[1].0.as_ref(), "x-result");
        assert_eq!(&headers[1].1[..], b"ok");
        assert_eq!(response_body, Bytes::from_static(b"created"));
    }
}
