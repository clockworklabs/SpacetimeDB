mod flat_csv;
pub(crate) mod serde;
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
use spacetimedb_client_api_messages::name::{parse_domain_name, DomainName};

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
    Name(DomainName),
}

impl NameOrIdentity {
    pub fn into_string(self) -> String {
        match self {
            NameOrIdentity::Identity(addr) => Identity::from(addr).to_hex().to_string(),
            NameOrIdentity::Name(name) => name.into(),
        }
    }

    pub fn name(&self) -> Option<&DomainName> {
        if let Self::Name(name) = self {
            Some(name)
        } else {
            None
        }
    }

    /// Resolve this [`NameOrIdentity`].
    ///
    /// If `self` is a [`NameOrIdentity::Identity`], the inner [`Identity`] is
    /// returned directly.
    ///
    /// Otherwise, if `self` is a [`NameOrIdentity::Name`], the [`Identity`] is
    /// looked up by that database name and returned.
    ///
    /// Errors are returned if name lookup fails.
    ///
    /// An `Ok` result is itself a [`Result`], which is `Err(DomainName)` if the
    /// given [`NameOrIdentity::Name`] is not registered in the SpacetimeDB DNS,
    /// i.e. no corresponding [`Identity`] exists.
    pub async fn try_resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> anyhow::Result<Result<Identity, &DomainName>> {
        Ok(match self {
            Self::Identity(identity) => Ok(Identity::from(*identity)),
            Self::Name(name) => ctx.lookup_database_identity(name.as_ref()).await?.ok_or(name),
        })
    }

    /// A variant of [`Self::try_resolve()`] which maps to a 404 (Not Found)
    /// response if `self` is a [`NameOrIdentity::Name`] for which no
    /// corresponding [`Identity`] is found.
    pub async fn resolve(&self, ctx: &(impl ControlStateReadAccess + ?Sized)) -> axum::response::Result<Identity> {
        self.try_resolve(ctx)
            .await
            .map_err(log_and_500)?
            .map_err(|name| (StatusCode::NOT_FOUND, format!("database name `{name}` not found")).into())
    }

    /// If `self` is a [`NameOrIdentity::Name`], looks up the name in the
    /// namespace registry (also known as "top level domain") and returns the
    /// owner identity if found.
    ///
    /// If the name is not found, returns a 404 (Not Found) error response.
    ///
    /// If `self` is a [`NameOrIdentity::Identity`], returns the identity.
    //
    // NOTE: Namespace (TLD) owner identities are also used as organization
    // identities.
    pub async fn resolve_namespace_owner(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<Identity> {
        match self {
            Self::Identity(identity) => Ok(Identity::from(*identity)),
            Self::Name(name) => ctx
                .lookup_namespace_owner(name.tld().as_str())
                .await
                .map_err(log_and_500)?
                .ok_or_else(|| (StatusCode::NOT_FOUND, format!("`{name}` not found")).into()),
        }
    }
}

impl<'de> ::serde::Deserialize<'de> for NameOrIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if let Ok(addr) = Identity::from_hex(&s) {
            Ok(NameOrIdentity::Identity(IdentityForUrl::from(addr)))
        } else {
            let name: DomainName = parse_domain_name(s).map_err(::serde::de::Error::custom)?;
            Ok(NameOrIdentity::Name(name))
        }
    }
}

impl fmt::Display for NameOrIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity(addr) => f.write_str(addr.into_inner().to_hex().as_str()),
            Self::Name(name) => f.write_str(name.as_ref()),
        }
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
