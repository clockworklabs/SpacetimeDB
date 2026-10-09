//! Automatic reconnect policy and credential refresh.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The upcoming automatic reconnect attempt (numbered starting at one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NextReconnect {
    pub attempt: u32,
    pub delay: Duration,
}

/// Bounds for exponential backoff with 50% jitter. Attempts are unlimited.
#[derive(Clone, Copy, Debug)]
pub struct AutomaticReconnectOptions {
    /// Defaults to one second; clamped to at least 500 milliseconds.
    pub min_delay: Duration,
    /// Defaults to 30 seconds; clamped to at least one second and `min_delay`.
    pub max_delay: Duration,
}
impl Default for AutomaticReconnectOptions {
    fn default() -> Self {
        Self {
            min_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}
impl AutomaticReconnectOptions {
    pub(crate) fn resolve(mut self) -> Self {
        let min = self.min_delay.max(Duration::from_millis(500));
        let max = self.max_delay.max(Duration::from_secs(1)).max(min);
        if min != self.min_delay || max != self.max_delay {
            log::warn!("Reconnect delays raised to safety bounds: {min:?}..={max:?}");
        }
        self.min_delay = min;
        self.max_delay = max;
        self
    }
    pub(crate) fn delay(self, attempt: u32, random: f64) -> Duration {
        let base = (self.min_delay.as_secs_f64() * 2f64.powi(attempt.saturating_sub(1).min(30) as i32))
            .min(self.max_delay.as_secs_f64());
        Duration::try_from_secs_f64(
            (base * (0.5 + random)).clamp(self.min_delay.as_secs_f64(), self.max_delay.as_secs_f64()),
        )
        .unwrap_or(self.max_delay)
    }
}

/// Supplies a fresh token for the same identity. Invoked only during reconnect,
/// when the retained JWT approaches expiry, cannot be inspected, or is rejected.
/// Provider errors are retried using the normal backoff policy.
pub trait TokenProvider: Send + Sync + 'static {
    fn token(&self) -> TokenFuture;
}

/// An asynchronous credential result. Browser futures need not be `Send`.
#[cfg(not(feature = "browser"))]
pub type TokenFuture = Pin<Box<dyn Future<Output = crate::Result<String>> + Send + 'static>>;
#[cfg(feature = "browser")]
pub type TokenFuture = Pin<Box<dyn Future<Output = crate::Result<String>> + 'static>>;

#[cfg(not(feature = "browser"))]
impl<F, Fut> TokenProvider for F
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::Result<String>> + Send + 'static,
{
    fn token(&self) -> TokenFuture {
        Box::pin(self())
    }
}
#[cfg(feature = "browser")]
impl<F, Fut> TokenProvider for F
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = crate::Result<String>> + 'static,
{
    fn token(&self) -> TokenFuture {
        Box::pin(self())
    }
}

pub(crate) fn now() -> SystemTime {
    #[cfg(not(feature = "browser"))]
    {
        SystemTime::now()
    }
    #[cfg(feature = "browser")]
    {
        UNIX_EPOCH + Duration::from_secs_f64(js_sys::Date::now() / 1000.0)
    }
}

pub(crate) fn token_needs_refresh(token: Option<&str>, now: SystemTime) -> bool {
    let claims = (|| {
        let payload = token?.split('.').nth(1)?;
        let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
        serde_json::from_slice::<serde_json::Value>(&bytes).ok()
    })();
    let Some(claims) = claims else {
        return true;
    };
    let Some(exp) = claims["exp"].as_f64().filter(|n| n.is_finite()) else {
        return true;
    };
    let margin = claims["iat"]
        .as_f64()
        .map(|iat| (exp - iat) * 0.05)
        .unwrap_or(0.0)
        .max(30.0);
    exp - now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() <= margin
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_bounds_and_saturation() {
        let policy = AutomaticReconnectOptions::default();
        for attempt in [1, 2, 3, 6, u32::MAX] {
            for random in [0.0, 0.5, 1.0] {
                let delay = policy.delay(attempt, random);
                assert!(delay >= policy.min_delay && delay <= policy.max_delay);
            }
        }
        assert_eq!(policy.delay(3, 0.5), Duration::from_secs(4));
        let policy = AutomaticReconnectOptions {
            min_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
        .resolve();
        assert_eq!(policy.min_delay, Duration::from_millis(500));
        assert_eq!(policy.max_delay, Duration::from_secs(1));
    }
    #[test]
    fn refresh_margin_and_unreadable_tokens() {
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        let token = |claims: &str| format!("header.{}.signature", URL_SAFE_NO_PAD.encode(claims));
        assert!(token_needs_refresh(None, now));
        assert!(token_needs_refresh(Some("invalid"), now));
        assert!(token_needs_refresh(Some(&token(r#"{"exp":1030,"iat":900}"#)), now));
        assert!(!token_needs_refresh(Some(&token(r#"{"exp":1031,"iat":900}"#)), now));
        assert!(token_needs_refresh(Some(&token(r#"{"exp":1100,"iat":-1000}"#)), now));
    }
}
