mod flat_csv;
pub(crate) mod serde;
pub mod websocket;

use core::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::net::IpAddr;
use std::ops::{Deref, DerefMut};

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::response::IntoResponse;
use bytestring::ByteString;
use futures::TryStreamExt;
use http::{HeaderName, HeaderValue, StatusCode};

use hyper::body::Body;
use spacetimedb::Identity;
use spacetimedb_client_api_messages::name::DatabaseName;
use tokio::task::{JoinError, JoinHandle};

use crate::routes::identity::IdentityForUrl;
use crate::{log_and_500, ControlStateReadAccess};

/// Returns a guard that runs async cleanup for `value` when dropped.
///
/// This is cancel-safe with respect to cancellation of the task holding the
/// guard: dropping the guard spawns the cleanup future in its own task instead
/// of trying to run async cleanup from `Drop`.
///
/// This does not guarantee that cleanup survives shutdown of the Tokio runtime,
/// process exit, or explicit abortion of the spawned cleanup task.
///
/// Dropping this guard calls [`tokio::spawn`], so it must be dropped from
/// within a Tokio runtime.
pub(crate) fn async_cleanup_guard<T, F, Fut>(value: T, cleanup: F) -> AsyncCleanupGuard<T, F, Fut>
where
    T: Send + 'static,
    F: FnOnce(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    AsyncCleanupGuard {
        value: Some(value),
        cleanup: Some(cleanup),
        future: PhantomData,
    }
}

/// Scope guard for values that require async cleanup.
///
/// Dropping the guard is cancel-safe for the guarded task: it moves the guarded
/// value into a newly spawned cleanup task. Drop does not wait for cleanup to
/// complete.
///
/// Call [`Self::cleanup`] on the normal path when the current task should wait
/// for cleanup. That method starts cleanup in a spawned task before awaiting it,
/// so cancelling the waiter does not cancel the cleanup task.
pub(crate) struct AsyncCleanupGuard<T, F, Fut>
where
    T: Send + 'static,
    F: FnOnce(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    value: Option<T>,
    cleanup: Option<F>,
    future: PhantomData<fn() -> Fut>,
}

impl<T, F, Fut> AsyncCleanupGuard<T, F, Fut>
where
    T: Send + 'static,
    F: FnOnce(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn spawn_cleanup(&mut self) -> JoinHandle<()> {
        let value = self.value.take().expect("cleanup value already taken");
        let cleanup = self.cleanup.take().expect("cleanup function already taken");
        tokio::spawn(cleanup(value))
    }

    /// Starts cleanup and waits for the cleanup task to finish.
    ///
    /// This is cancel-safe with respect to cancellation of the caller: cleanup
    /// is spawned before this method awaits, so dropping this future after its
    /// first poll drops only the wait for completion, not the cleanup itself.
    ///
    /// This is not cancel-safe against explicit abortion of the returned
    /// cleanup task by the runtime or against runtime shutdown.
    pub(crate) async fn cleanup(mut self) -> Result<(), JoinError> {
        self.spawn_cleanup().await
    }
}

impl<T, F, Fut> Deref for AsyncCleanupGuard<T, F, Fut>
where
    T: Send + 'static,
    F: FnOnce(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref().expect("cleanup value already taken")
    }
}

impl<T, F, Fut> DerefMut for AsyncCleanupGuard<T, F, Fut>
where
    T: Send + 'static,
    F: FnOnce(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().expect("cleanup value already taken")
    }
}

impl<T, F, Fut> Drop for AsyncCleanupGuard<T, F, Fut>
where
    T: Send + 'static,
    F: FnOnce(T) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn drop(&mut self) {
        if let (Some(value), Some(cleanup)) = (self.value.take(), self.cleanup.take()) {
            tokio::spawn(cleanup(value));
        }
    }
}

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
        // X-Forwarded-For is a comma-separated chain. For a single-hop
        // proxy there is no comma; take the first IP either way.
        let first = val.split(',').next().unwrap_or(val).trim();
        let ip = first.parse().map_err(|_| headers::Error::invalid())?;
        Ok(XForwardedFor(ip))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.to_string().try_into().unwrap()])
    }
}

#[derive(Clone, Debug)]
pub enum NameOrIdentity {
    Identity(IdentityForUrl),
    Name(DatabaseName),
}

impl NameOrIdentity {
    pub fn into_string(self) -> String {
        match self {
            NameOrIdentity::Identity(addr) => Identity::from(addr).to_hex().to_string(),
            NameOrIdentity::Name(name) => name.into(),
        }
    }

    pub fn name(&self) -> Option<&DatabaseName> {
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
    /// looked up by that name in the SpacetimeDB DNS and returned.
    ///
    /// Errors are returned if the DNS lookup fails.
    ///
    /// An `Ok` result is itself a [`Result`], which is `Err(DatabaseName)` if the
    /// given [`NameOrIdentity::Name`] is not registered in the SpacetimeDB DNS,
    /// i.e. no corresponding [`Identity`] exists.
    pub async fn try_resolve(
        &self,
        ctx: &(impl ControlStateReadAccess + ?Sized),
    ) -> anyhow::Result<Result<Identity, &DatabaseName>> {
        Ok(match self {
            Self::Identity(identity) => Ok(Identity::from(*identity)),
            Self::Name(name) => ctx.lookup_database_identity(name.as_ref()).await?.ok_or(name),
        })
    }

    /// A variant of [`Self::try_resolve()`] which maps to a 404 (Not Found)
    /// response if `self` is a [`NameOrIdentity::Name`] for which no
    /// corresponding [`Identity`] is found in the SpacetimeDB DNS.
    pub async fn resolve(&self, ctx: &(impl ControlStateReadAccess + ?Sized)) -> axum::response::Result<Identity> {
        self.try_resolve(ctx)
            .await
            .map_err(log_and_500)?
            .map_err(|name| (StatusCode::NOT_FOUND, format!("`{name}` not found")).into())
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
                .lookup_namespace_owner(name.as_ref())
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
            let name: DatabaseName = s.try_into().map_err(::serde::de::Error::custom)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use headers::Header;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::{oneshot, Notify};

    fn decode_one(raw: &str) -> Result<XForwardedFor, headers::Error> {
        let val = HeaderValue::from_str(raw).unwrap();
        let values = [val];
        XForwardedFor::decode(&mut values.iter())
    }

    #[test]
    fn decodes_single_ip() {
        // Single-hop proxies (e.g. nginx inserting the client IP) emit
        // a value with no comma. This must succeed.
        let got = decode_one("10.0.0.1").expect("single IP should decode");
        assert_eq!(got.0, "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn decodes_chain_takes_first() {
        let got = decode_one("10.0.0.1, 192.168.1.1, 172.16.0.1").expect("chain should decode");
        assert_eq!(got.0, "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn decodes_chain_trims_whitespace() {
        let got = decode_one("   10.0.0.1   , 192.168.1.1").expect("chain should decode");
        assert_eq!(got.0, "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn rejects_non_ip() {
        assert!(decode_one("not-an-ip").is_err());
        assert!(decode_one("not-an-ip, 10.0.0.1").is_err());
    }

    #[tokio::test]
    async fn async_cleanup_guard_runs_cleanup_when_dropped() {
        let (tx, rx) = oneshot::channel();

        drop(async_cleanup_guard((), move |()| async move {
            tx.send(()).unwrap();
        }));

        rx.await.expect("cleanup should run");
    }

    #[tokio::test]
    async fn async_cleanup_guard_cleanup_waits_for_cleanup() {
        let cleaned_up = Arc::new(AtomicBool::new(false));
        let cleanup_started = Arc::new(Notify::new());
        let finish_cleanup = Arc::new(Notify::new());
        let cleaned_up_for_guard = Arc::clone(&cleaned_up);
        let cleanup_started_for_guard = Arc::clone(&cleanup_started);
        let finish_cleanup_for_guard = Arc::clone(&finish_cleanup);

        let cleanup = tokio::spawn(
            async_cleanup_guard((), move |()| async move {
                cleanup_started_for_guard.notify_one();
                finish_cleanup_for_guard.notified().await;
                cleaned_up_for_guard.store(true, Ordering::Release);
            })
            .cleanup(),
        );

        cleanup_started.notified().await;
        assert!(!cleaned_up.load(Ordering::Acquire));

        finish_cleanup.notify_one();
        cleanup
            .await
            .expect("cleanup join task should not panic")
            .expect("cleanup task should not panic");
        assert!(cleaned_up.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn async_cleanup_guard_cleanup_continues_if_waiter_is_aborted() {
        let cleaned_up = Arc::new(AtomicBool::new(false));
        let cleanup_started = Arc::new(Notify::new());
        let finish_cleanup = Arc::new(Notify::new());
        let cleaned_up_for_guard = Arc::clone(&cleaned_up);
        let cleanup_started_for_guard = Arc::clone(&cleanup_started);
        let finish_cleanup_for_guard = Arc::clone(&finish_cleanup);

        let cleanup = tokio::spawn(
            async_cleanup_guard((), move |()| async move {
                cleanup_started_for_guard.notify_one();
                finish_cleanup_for_guard.notified().await;
                cleaned_up_for_guard.store(true, Ordering::Release);
            })
            .cleanup(),
        );

        cleanup_started.notified().await;
        cleanup.abort();
        assert!(cleanup.await.unwrap_err().is_cancelled());

        finish_cleanup.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !cleaned_up.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cleanup should continue after waiter abort");
    }
}
