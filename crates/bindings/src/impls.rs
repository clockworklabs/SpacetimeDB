use crate::FilterableValue;
use spacetimedb_lib::{Address, Identity};

macro_rules! impl_primitives {
    ($($t:ty),*) => {
        $(
            impl FilterableValue for $t {}
        )*
    };
}

impl_primitives![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, bool, String, Identity, Address];
