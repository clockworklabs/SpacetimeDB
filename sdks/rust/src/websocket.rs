//! Low-level WebSocket plumbing.
//!
//! This module is internal, and may incompatibly change without warning.

#[cfg(feature = "browser")]
#[path = "websocket/browser.rs"]
mod browser;
#[cfg(not(feature = "browser"))]
#[path = "websocket/native.rs"]
mod native;
#[path = "websocket/shared.rs"]
mod shared;

pub(crate) use self::shared::{WsConnection, WsError, WsParams};
