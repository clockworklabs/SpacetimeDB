use std::borrow::Cow;

pub mod prometheus_handle;

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

pub use sled::IVec;
