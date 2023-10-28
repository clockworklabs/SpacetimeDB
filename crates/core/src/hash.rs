pub use spacetimedb_sats::hash::*;

pub trait ToHexString {
    fn to_hex_string(&self) -> String;
}

impl ToHexString for Hash {
    fn to_hex_string(&self) -> String {
        self.to_hex()
    }
}
