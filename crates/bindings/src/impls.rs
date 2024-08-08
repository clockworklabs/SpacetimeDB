use crate::FilterableValue;
use spacetimedb_lib::{
    sats::{i256, u256},
    Address, Identity,
};

macro_rules! impl_primitives {
    ($($t:ty),*) => {
        $(
            impl FilterableValue for $t {}
        )*
    };
}

impl_primitives![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, u256, i256, bool, String, Identity, Address];
