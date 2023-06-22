/// Error code for "No such table".
pub const NOTAB: u16 = 1;

/// Error code for value/range not being found in a table.
pub const LOOKUP: u16 = 2;

/// Error code for when a unique constraint is violated.
pub const EXISTS: u16 = 3;

macro_rules! errnos {
    ($mac:ident) => {
        $mac! {
            NOTAB => "No such table",
            LOOKUP => "Value or range provided not found in table",
            EXISTS => "Value with given unique identifier already exists",
        }
    };
}
