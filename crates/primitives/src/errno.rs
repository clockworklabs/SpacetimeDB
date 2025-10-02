//! Error numbers for the wasm abi.

use core::num::NonZeroU16;

/// Takes a macro that expects `$($err_name:ident($errno:literal, $errmsg:literal),)*` and invokes
/// it with the errnos defined in this module.
#[macro_export]
macro_rules! errnos {
    ($mac:ident) => {
        $mac!(
            HOST_CALL_FAILURE(1, "ABI called by host returned an error"),
            NOT_IN_TRANSACTION(2, "ABI call can only be made while in a transaction"),
            BSATN_DECODE_ERROR(3, "Couldn't decode the BSATN to the expected type"),
            NO_SUCH_TABLE(4, "No such table"),
            NO_SUCH_INDEX(5, "No such index"),
            NO_SUCH_ITER(6, "The provided row iterator is not valid"),
            NO_SUCH_CONSOLE_TIMER(7, "The provided console timer does not exist"),
            NO_SUCH_BYTES(8, "The provided bytes source or sink is not valid"),
            NO_SPACE(9, "The provided sink has no more space left"),
            BUFFER_TOO_SMALL(11, "The provided buffer is not large enough to store the data"),
            UNIQUE_ALREADY_EXISTS(12, "Value with given unique identifier already exists"),
            SCHEDULE_AT_DELAY_TOO_LONG(13, "Specified delay in scheduling row was too long"),
            INDEX_NOT_UNIQUE(14, "The index was not unique"),
            NO_SUCH_ROW(15, "The row was not found, e.g., in an update call"),
            AUTO_INC_OVERFLOW(16, "The auto-increment sequence overflowed"),
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
