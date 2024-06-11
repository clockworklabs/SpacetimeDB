//! Error numbers for the wasm abi.

use core::num::NonZeroU16;

/// Takes a macro that expects `$($err_name:ident($errno:literal, $errmsg:literal),)*` and invokes
/// it with the errnos defined in this module.
#[macro_export]
macro_rules! errnos {
    ($mac:ident) => {
        $mac!(
            NO_SUCH_TABLE(1, "No such table"),
            LOOKUP_NOT_FOUND(2, "Value or range provided not found in table"),
            UNIQUE_ALREADY_EXISTS(3, "Value with given unique identifier already exists"),
            BUFFER_TOO_SMALL(4, "The provided buffer is not large enough to store the data"),
        );
    };
}

const fn nz(n: u16) -> NonZeroU16 {
    match NonZeroU16::new(n) {
        Some(n) => n,
        None => panic!(),
    }
}

macro_rules! def_errnos {
    ($($err_name:ident($errno:literal, $errmsg:literal),)*) => {
        $(#[doc = $errmsg] pub const $err_name: NonZeroU16 = nz($errno);)*

        /// Get the error message for an error number, if it exists.
        pub const fn strerror(num: NonZeroU16) -> Option<&'static str> {
            match num.get() {
                $($errno => Some($errmsg),)*
                _ => None,
            }
        }
    };
}
errnos!(def_errnos);
