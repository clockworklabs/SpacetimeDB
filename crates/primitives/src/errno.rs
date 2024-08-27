//! Error numbers for the wasm abi.

use core::num::NonZeroU16;

/// Takes a macro that expects `$($err_name:ident($errno:literal, $errmsg:literal),)*` and invokes
/// it with the errnos defined in this module.
#[macro_export]
macro_rules! errnos {
    ($mac:ident) => {
        $mac!(
            // TODO(1.0): remove this.
            LOOKUP_NOT_FOUND(2, "Value or range provided not found in table"),
            HOST_CALL_FAILURE(1, "ABI called by host returned an error"),
            NO_SUCH_TABLE(4, "No such table"),
            NO_SUCH_BYTES(8, "The provided bytes source or sink is not valid"),
            NO_SPACE(9, "The provided sink has no more space left"),
            BUFFER_TOO_SMALL(11, "The provided buffer is not large enough to store the data"),
            UNIQUE_ALREADY_EXISTS(12, "Value with given unique identifier already exists"),
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
