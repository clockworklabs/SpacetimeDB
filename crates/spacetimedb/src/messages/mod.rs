// NOTE: Protobuf serialization is not guaranteed to be deterministic
// we will eventually have to replace protobuf if we use content
// addressing.
mod spacetimedb;
pub use self::spacetimedb::*;
