use crate::{de, impl_deserialize, impl_serialize, impl_st, AlgebraicType};
pub use spacetimedb_primitives::{ColumnAttribute, Constraints};

impl_deserialize!([] ColumnAttribute, de =>
    Self::from_bits(de.deserialize_u8()?)
        .ok_or_else(|| de::Error::custom("invalid bitflags for `ColumnAttribute`"))
);
impl_serialize!([] ColumnAttribute, (self, ser) => ser.serialize_u8(self.bits()));
impl_st!([] ColumnAttribute, AlgebraicType::U8);

impl_deserialize!([] Constraints, de => Self::try_from(de.deserialize_u8()?)
    .map_err(|_| de::Error::custom("invalid bitflags for `Constraints`"))
);
impl_serialize!([] Constraints, (self, ser) => ser.serialize_u8(self.bits()));
impl_st!([] Constraints, AlgebraicType::U8);
