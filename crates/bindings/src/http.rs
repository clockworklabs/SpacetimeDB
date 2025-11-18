//! Types and utilities for performing HTTP requests in [procedures](crate::procedure).
//!
//! Perform an HTTP request using methods on [`crate::ProcedureContext::http`],
//! which is of type [`HttpClient`].
//! The [`get`](HttpClient::get) helper can be used for simple `GET` requests,
//! while [`send`](HttpClient::send) allows more complex requests with headers, bodies and other methods.

pub use http::{Request, Response};
pub use spacetimedb_lib::http::{Body, Error, Timeout};

use crate::{
    rt::{read_bytes_source_into, BytesSource},
    IterBuf,
};
use spacetimedb_lib::{bsatn, de::Deserialize, http as st_http};

/// Allows performing
#[non_exhaustive]
pub struct HttpClient {}

impl HttpClient {
    /// Send the HTTP request `request` and wait for its response.
    ///
    /// For simple `GET` requests with no headers, use [`HttpClient::get`] instead.
    // TODO(docs): expand docs
    pub fn send<B: Into<Body>>(&self, request: Request<B>) -> Result<Response<Body>, Error> {
        let request = st_http::Request::from(request);
        let request = bsatn::to_vec(&request).expect("Failed to BSATN-serialize `spacetimedb_lib::http::Request`");

        fn read_output<T: for<'a> Deserialize<'a> + 'static>(source: BytesSource) -> T {
            let mut buf = IterBuf::take();
            read_bytes_source_into(source, &mut buf);
            bsatn::from_slice::<T>(&buf)
                .unwrap_or_else(|err| panic!("Failed to BSATN-deserialize `{}`: {err:#?}", std::any::type_name::<T>()))
        }

        match spacetimedb_bindings_sys::procedure::http_request(&request) {
            Ok(response_source) => {
                let response = read_output::<st_http::Response>(response_source);
                let response = http::Response::<Body>::from(response);
                Ok(response)
            }
            Err(err_source) => {
                let error = read_output::<st_http::Error>(err_source);
                Err(error)
            }
        }
    }

    /// Send a `GET` request to `uri` with no headers.
    ///
    /// Blocks procedure execution for the duration of the HTTP request.
    pub fn get<Uri>(&self, uri: Uri) -> Result<Response<Body>, Error>
    where
        Uri: TryInto<http::Uri>,
        <Uri as TryInto<http::Uri>>::Error: Into<http::Error>,
    {
        self.send(
            http::Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .map_err(|err| Error::from_display(&err))?,
        )
    }
}
