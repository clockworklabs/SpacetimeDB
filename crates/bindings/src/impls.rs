use crate::{FilterableValue, IsSequenceTrigger};
use spacetimedb_lib::{
    sats::{i256, u256},
    Address, Identity,
};

macro_rules! impl_filterable_value {
    ($($t:ty),*) => {
        $(
            impl FilterableValue for $t {}
        )*
    };
}

impl_filterable_value![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, u256, i256, bool, String, Identity, Address];

macro_rules! impl_is_seq_trigger {
    ($($t:ty),*) => {
        $(
            impl IsSequenceTrigger for $t {
                fn is_sequence_trigger(&self) -> bool { *self == 0 }
            }
        )*
    };
}

impl_is_seq_trigger![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128];

impl IsSequenceTrigger for i256 {
    fn is_sequence_trigger(&self) -> bool {
        *self == Self::ZERO
    }
}

impl IsSequenceTrigger for u256 {
    fn is_sequence_trigger(&self) -> bool {
        *self == Self::ZERO
    }
}
