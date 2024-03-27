/// Error code for "No such table".
pub const NO_SUCH_TABLE: u16 = 1;

/// Error code for value/range not being found in a table.
pub const LOOKUP_NOT_FOUND: u16 = 2;

/// Error code for when a unique constraint is violated.
pub const UNIQUE_ALREADY_EXISTS: u16 = 3;

/// The provided buffer is not large enough to store the data.
pub const BUFFER_TOO_SMALL: u16 = 4;

macro_rules! errnos {
    ($mac:ident) => {
        $mac! {
            NO_SUCH_TABLE => "No such table",
            LOOKUP_NOT_FOUND => "Value or range provided not found in table",
            UNIQUE_ALREADY_EXISTS => "Value with given unique identifier already exists",
            BUFFER_TOO_SMALL => "The provided buffer is not large enough to store the data",
        }
    };
}
