use std::borrow::Cow;

pub mod prometheus_handle;

mod future_queue;
pub mod lending_pool;
pub mod notify_once;

pub use future_queue::{future_queue, FutureQueue};

pub trait ResultInspectExt<T, E> {
    fn inspect_err_(self, f: impl FnOnce(&E)) -> Self;
}
impl<T, E> ResultInspectExt<T, E> for Result<T, E> {
    #[inline]
    fn inspect_err_(self, f: impl FnOnce(&E)) -> Self {
        if let Err(e) = &self {
            f(e)
        }
        self
    }
}

pub(crate) fn string_from_utf8_lossy_owned(v: Vec<u8>) -> String {
    match String::from_utf8_lossy(&v) {
        // SAFETY: from_utf8_lossy() returned Borrowed, which means the original buffer is valid utf8
        Cow::Borrowed(_) => unsafe { String::from_utf8_unchecked(v) },
        Cow::Owned(s) => s,
    }
}

#[derive(Clone)]
pub enum AnyBytes {
    Bytes(bytes::Bytes),
    IVec(sled::IVec),
}

impl From<bytes::Bytes> for AnyBytes {
    fn from(b: bytes::Bytes) -> Self {
        Self::Bytes(b)
    }
}
impl From<Vec<u8>> for AnyBytes {
    fn from(b: Vec<u8>) -> Self {
        Self::Bytes(b.into())
    }
}
impl From<sled::IVec> for AnyBytes {
    fn from(b: sled::IVec) -> Self {
        Self::IVec(b)
    }
}

impl AsRef<[u8]> for AnyBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl std::ops::Deref for AnyBytes {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            AnyBytes::Bytes(b) => b,
            AnyBytes::IVec(b) => b,
        }
    }
}
