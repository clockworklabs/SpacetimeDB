mod flat_csv;
pub mod websocket;

use core::fmt;
use std::net::IpAddr;

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::response::IntoResponse;
use bytestring::ByteString;
use http::{HeaderName, HeaderValue, StatusCode};

use spacetimedb::address::Address;
use spacetimedb_lib::address::AddressForUrl;
use spacetimedb_lib::name::DomainName;

use crate::routes::database::DomainParsingRejection;
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
pub enum NameOrAddress {
    Address(AddressForUrl),
    Name(String),
}

impl NameOrAddress {
    pub fn into_string(self) -> String {
        match self {
            NameOrAddress::Address(addr) => Address::from(addr).to_hex().to_string(),
            NameOrAddress::Name(name) => name,
        }
    }

    /// Resolve this [`NameOrAddress`].
    ///
    /// If `self` is a [`NameOrAddress::Address`], the inner [`Address`] is
    /// returned in a [`ResolvedAddress`] without a [`DomainName`].
    ///
    /// Otherwise, if `self` is a [`NameOrAddress::Name`], the [`Address`] is
    /// looked up by that name in the SpacetimeDB DNS and returned in a
    /// [`ResolvedAddress`] alongside `Some` [`DomainName`].
    ///
    /// Errors are returned if [`NameOrAddress::Name`] cannot be parsed into a
    /// [`DomainName`], or the DNS lookup fails.
    ///
    /// An `Ok` result is itself a [`Result`], which is `Err(DomainName)` if the
    /// given [`NameOrAddress::Name`] is not registered in the SpacetimeDB DNS,
    /// i.e. no corresponding [`Address`] exists.
    pub async fn try_resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<Result<ResolvedAddress, DomainName>> {
        Ok(match self {
            Self::Address(addr) => Ok(ResolvedAddress {
                address: Address::from(*addr),
                domain: None,
            }),
            Self::Name(name) => {
                let domain = name.parse().map_err(DomainParsingRejection)?;
                let address = ctx.lookup_address(&domain).map_err(log_and_500)?;
                match address {
                    Some(address) => Ok(ResolvedAddress {
                        address,
                        domain: Some(domain),
                    }),
                    None => Err(domain),
                }
            }
        })
    }

    /// A variant of [`Self::try_resolve()`] which maps to a 400 (Bad Request)
    /// response if `self` is a [`NameOrAddress::Name`] for which no
    /// corresponding [`Address`] is found in the SpacetimeDB DNS.
    pub async fn resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<ResolvedAddress> {
        self.try_resolve(ctx).await?.map_err(|_| StatusCode::BAD_REQUEST.into())
    }
}

impl<'de> serde::Deserialize<'de> for NameOrAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|s| {
            if let Ok(addr) = Address::from_hex(&s) {
                NameOrAddress::Address(AddressForUrl::from(addr))
            } else {
                NameOrAddress::Name(s)
            }
        })
    }
}

impl fmt::Display for NameOrAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(addr) => f.write_str(&Address::from(*addr).to_hex()),
            Self::Name(name) => f.write_str(name),
        }
    }
}

/// A resolved [`NameOrAddress`].
///
/// Constructed by [`NameOrAddress::try_resolve()`].
pub struct ResolvedAddress {
    address: Address,
    domain: Option<DomainName>,
}

impl ResolvedAddress {
    pub fn address(&self) -> &Address {
        &self.address
    }

    pub fn domain(&self) -> Option<&DomainName> {
        self.domain.as_ref()
    }
}

impl From<ResolvedAddress> for Address {
    fn from(value: ResolvedAddress) -> Self {
        value.address
    }
}

impl From<ResolvedAddress> for (Address, Option<DomainName>) {
    fn from(ResolvedAddress { address, domain }: ResolvedAddress) -> Self {
        (address, domain)
    }
}
