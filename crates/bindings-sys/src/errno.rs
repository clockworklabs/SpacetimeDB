pub const NOTAB: u16 = 1;
pub const LOOKUP: u16 = 2;
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
