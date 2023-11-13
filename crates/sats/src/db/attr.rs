use crate::{de, impl_deserialize, impl_serialize};
pub use spacetimedb_primitives::ColumnIndexAttribute;

impl_deserialize!([] ColumnIndexAttribute, de =>
    Self::from_bits(de.deserialize_u8()?)
        .ok_or_else(|| de::Error::custom("invalid bitflags for `ColumnIndexAttribute`"))
);

impl_serialize!([] ColumnIndexAttribute, (self, ser) => ser.serialize_u8(self.bits()));
