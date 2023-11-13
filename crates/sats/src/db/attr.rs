use crate::{de, impl_deserialize, impl_serialize};
pub use spacetimedb_primitives::ColumnAttribute;

impl_deserialize!([] ColumnAttribute, de =>
    Self::from_bits(de.deserialize_u8()?)
        .ok_or_else(|| de::Error::custom("invalid bitflags for `ColumnAttribute`"))
);

impl_serialize!([] ColumnAttribute, (self, ser) => ser.serialize_u8(self.bits()));
