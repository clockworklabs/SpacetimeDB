mod flat_csv;
pub mod websocket;

use core::fmt;
use std::net::IpAddr;

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::response::IntoResponse;
use bytestring::ByteString;
use futures::TryStreamExt;
use http::{HeaderName, HeaderValue, StatusCode};

use hyper::body::Body;
use spacetimedb::Identity;
use spacetimedb_client_api_messages::name::DomainName;

use crate::routes::domain::DomainParsingRejection;
use crate::routes::identity::IdentityForUrl;
use crate::{log_and_500, ControlStateReadAccess};

pub struct ByteStringBody(pub ByteString);

#[async_trait::async_trait]
impl<S: Send + Sync> FromRequest<S> for ByteStringBody {
    type Rejection = axum::response::Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(IntoResponse::into_response)?;

        let string = bytes
            .try_into()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Request body didn't contain valid UTF-8").into_response())?;

        Ok(ByteStringBody(string))
    }
}

pub struct XForwardedFor(pub IpAddr);

impl headers::Header for XForwardedFor {
    fn name() -> &'static HeaderName {
        static NAME: HeaderName = HeaderName::from_static("x-forwarded-for");
        &NAME
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(values: &mut I) -> Result<Self, headers::Error> {
        let val = values.next().ok_or_else(headers::Error::invalid)?;
        let val = val.to_str().map_err(|_| headers::Error::invalid())?;
        let (first, _) = val.split_once(',').ok_or_else(headers::Error::invalid)?;
        let ip = first.trim().parse().map_err(|_| headers::Error::invalid())?;
        Ok(XForwardedFor(ip))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.to_string().try_into().unwrap()])
    }
}

#[derive(Clone, Debug)]
pub enum NameOrIdentity {
    Identity(IdentityForUrl),
    Name(String),
}

impl NameOrIdentity {
    pub fn into_string(self) -> String {
        match self {
            NameOrIdentity::Identity(addr) => Identity::from(addr).to_hex().to_string(),
            NameOrIdentity::Name(name) => name,
        }
    }

    /// Resolve this [`NameOrIdentity`].
    ///
    /// If `self` is a [`NameOrIdentity::Identity`], the inner [`Identity`] is
    /// returned in a [`ResolvedIdentity`] without a [`DomainName`].
    ///
    /// Otherwise, if `self` is a [`NameOrIdentity::Name`], the [`Identity`] is
    /// looked up by that name in the SpacetimeDB DNS and returned in a
    /// [`ResolvedIdentity`] alongside `Some` [`DomainName`].
    ///
    /// Errors are returned if [`NameOrIdentity::Name`] cannot be parsed into a
    /// [`DomainName`], or the DNS lookup fails.
    ///
    /// An `Ok` result is itself a [`Result`], which is `Err(DomainName)` if the
    /// given [`NameOrIdentity::Name`] is not registered in the SpacetimeDB DNS,
    /// i.e. no corresponding [`Identity`] exists.
    pub async fn try_resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<Result<ResolvedIdentity, DomainName>> {
        Ok(match self {
            Self::Identity(identity) => Ok(ResolvedIdentity {
                identity: Identity::from(*identity),
                domain: None,
            }),
            Self::Name(name) => {
                let domain = name.parse().map_err(|_| DomainParsingRejection)?;
                let identity = ctx.lookup_identity(&domain).map_err(log_and_500)?;
                match identity {
                    Some(identity) => Ok(ResolvedIdentity {
                        identity,
                        domain: Some(domain),
                    }),
                    None => Err(domain),
                }
            }
        })
    }

    /// A variant of [`Self::try_resolve()`] which maps to a 404 (Not Found)
    /// response if `self` is a [`NameOrIdentity::Name`] for which no
    /// corresponding [`Identity`] is found in the SpacetimeDB DNS.
    pub async fn resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<ResolvedIdentity> {
        self.try_resolve(ctx).await?.map_err(|_| StatusCode::NOT_FOUND.into())
    }
}

impl<'de> serde::Deserialize<'de> for NameOrIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|s| {
            if let Ok(addr) = Identity::from_hex(&s) {
                NameOrIdentity::Identity(IdentityForUrl::from(addr))
            } else {
                NameOrIdentity::Name(s)
            }
        })
    }
}

impl fmt::Display for NameOrIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity(addr) => f.write_str(addr.into_inner().to_hex().as_str()),
            Self::Name(name) => f.write_str(name),
        }
    }
}

/// A resolved [`NameOrIdentity`].
///
/// Constructed by [`NameOrIdentity::try_resolve()`].
pub struct ResolvedIdentity {
    identity: Identity,
    domain: Option<DomainName>,
}

impl ResolvedIdentity {
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn domain(&self) -> Option<&DomainName> {
        self.domain.as_ref()
    }
}

impl From<ResolvedIdentity> for Identity {
    fn from(value: ResolvedIdentity) -> Self {
        value.identity
    }
}

impl From<ResolvedIdentity> for (Identity, Option<DomainName>) {
    fn from(ResolvedIdentity { identity, domain }: ResolvedIdentity) -> Self {
        (identity, domain)
    }
}

pub struct EmptyBody;

#[async_trait::async_trait]
impl<S> FromRequest<S> for EmptyBody {
    type Rejection = axum::response::Response;
    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let body = req.into_body();
        if body.is_end_stream() {
            return Ok(Self);
        }

        if body
            .into_data_stream()
            .try_any(|data| futures::future::ready(!data.is_empty()))
            .await
            .map_err(|_| (StatusCode::BAD_REQUEST, "Failed to buffer the request body").into_response())?
        {
            return Err((StatusCode::BAD_REQUEST, "body must be empty").into_response());
        }
        Ok(Self)
    }
}
