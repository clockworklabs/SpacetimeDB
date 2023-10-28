//! Defines all the typed database objects & schemas
use crate::db::attr::ColumnAttribute;
use crate::{de, impl_deserialize, impl_serialize};

pub mod attr;
pub mod auth;
pub mod def;
pub mod error;

impl TryFrom<u8> for ColumnAttribute {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Self::from_bits(v).ok_or(())
    }
}

impl_deserialize!([] ColumnAttribute, de =>  Self::from_bits(de.deserialize_u8()?)
            .ok_or_else(|| de::Error::custom("invalid bitflags for ColumnIndexAttribute")));

impl_serialize!([] ColumnAttribute, (self, ser) => ser.serialize_u8(self.bits()));
