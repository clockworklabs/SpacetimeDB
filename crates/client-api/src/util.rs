mod flat_csv;
pub mod websocket;

use std::net::IpAddr;

use axum::body::{Bytes, HttpBody};
use axum::extract::FromRequest;
use axum::headers;
use axum::response::IntoResponse;
use bytestring::ByteString;
use http::{HeaderName, HeaderValue, Request, StatusCode};
use spacetimedb::address::Address;
use spacetimedb_lib::name::DomainName;

use crate::routes::database::DomainParsingRejection;
use crate::{log_and_500, ControlStateReadAccess};

pub struct ByteStringBody(pub ByteString);

#[async_trait::async_trait]
impl<S, B> FromRequest<S, B> for ByteStringBody
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<axum::BoxError>,
    S: Send + Sync,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
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
    Address(Address),
    Name(String),
}

impl NameOrAddress {
    pub fn into_string(self) -> String {
        match self {
            NameOrAddress::Address(addr) => addr.to_hex(),
            NameOrAddress::Name(name) => name,
        }
    }

    /// If this [`NameOrAddress`] is a [`NameOrAddress::Name`], try to parse it
    /// into a [`DomainName`] and then try to find a corresponding [`Address`].
    /// If it is a [`NameOrAddress::Address`], just return the [`Address`].
    ///
    /// An error is returned if parsing the name variant into a [`DomainName`]
    /// fails, or an I/O error occurs.
    ///
    /// An `Ok` result contains another [`Result`], which covers the following
    /// cases:
    ///
    /// * `Ok((address, None))` if we resolved a [`NameOrAddress::Address`].
    /// * `Ok((address, Some(domain)))` if we successfully resolved a
    ///   [`NameOrAddress::Name`].
    /// * `Err(domain)` if we successfully parsed a [`NameOrAddress::Name`], but
    ///   no corresponding [`Address`] entry was found.
    ///
    pub async fn try_resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<Result<(Address, Option<DomainName>), DomainName>> {
        Ok(match self {
            NameOrAddress::Address(addr) => Ok((*addr, None)),
            NameOrAddress::Name(name) => {
                let domain = name.parse().map_err(DomainParsingRejection)?;
                match ctx.lookup_address(&domain).map_err(log_and_500)? {
                    Some(addr) => Ok((addr, Some(domain))),
                    None => Err(domain),
                }
            }
        })
    }

    pub async fn resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> axum::response::Result<(Address, Option<DomainName>)> {
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
                NameOrAddress::Address(addr)
            } else {
                NameOrAddress::Name(s)
            }
        })
    }
}
