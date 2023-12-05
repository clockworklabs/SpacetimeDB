use spacetimedb_lib::{Address, Identity};

use super::PrimaryKey;
use crate::sats::data_key::DataKey;
use crate::sats::hash::Hash;
use crate::{FilterableValue, UniqueValue};

macro_rules! impl_primitives {
    (uniq { $($t:ty => $x:ident,)*}) => {
        $(
            impl FilterableValue for $t {}
            impl UniqueValue for $t {
                fn into_primarykey(self) -> PrimaryKey {
                    todo!() // idk what this is
                }
            }
        )*
    };
}

impl FilterableValue for u8 {}
impl UniqueValue for u8 {
    fn into_primarykey(self) -> PrimaryKey {
        todo!() // idk what this is
    }
}

impl_primitives! {
    uniq {
        i8 => I8,
        u16 => U16,
        i16 => I16,
        u32 => U32,
        i32 => I32,
        u64 => U64,
        i64 => I64,
        u128 => U128,
        i128 => I128,
        bool => Bool,
        String => String,
    }
}

impl FilterableValue for Hash {}
impl UniqueValue for Hash {
    fn into_primarykey(self) -> PrimaryKey {
        PrimaryKey {
            data_key: DataKey::Hash(self),
        }
    }
}

impl FilterableValue for Identity {}
impl UniqueValue for Identity {
    fn into_primarykey(self) -> PrimaryKey {
        todo!() // idk what this is
    }
}

impl FilterableValue for Address {}
impl UniqueValue for Address {
    fn into_primarykey(self) -> PrimaryKey {
        todo!()
    }
}
