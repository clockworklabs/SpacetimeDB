use crate::{de, impl_deserialize, impl_serialize};
pub use spacetimedb_primitives::ColumnAttribute;
pub use spacetimedb_primitives::Constraints;

impl_deserialize!([] ColumnAttribute, de =>
    Self::from_bits(de.deserialize_u8()?)
        .ok_or_else(|| de::Error::custom("invalid bitflags for `ColumnAttribute`"))
);

impl_serialize!([] ColumnAttribute, (self, ser) => ser.serialize_u8(self.bits()));

impl_deserialize!([] Constraints, de => Self::try_from(de.deserialize_u8()?)
    .map_err(|_| de::Error::custom("invalid bitflags for `Constraints`"))
);
impl_serialize!([] Constraints, (self, ser) => ser.serialize_u8(self.bits()));
