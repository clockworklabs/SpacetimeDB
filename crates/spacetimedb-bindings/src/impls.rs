use spacetimedb_lib::sats::BuiltinType;
use spacetimedb_lib::{DataKey, Hash};

use super::{PrimaryKey, TypeDef};
use crate::{FilterableValue, SpacetimeType, UniqueValue};

macro_rules! impl_primitives {
    ($($t:ty => $x:ident,)*) => {
        $(
            impl SpacetimeType for $t {
                fn get_schema() -> TypeDef {
                    TypeDef::$x
                }
            }
        )*
    };
    (uniq { $($t:ty => $x:ident,)*}) => {
        impl_primitives!($($t => $x,)*);
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

impl SpacetimeType for u8 {
    fn get_schema() -> TypeDef {
        TypeDef::U8
    }
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

impl_primitives! {
    f32 => F32,
    f64 => F64,
}

impl SpacetimeType for () {
    fn get_schema() -> TypeDef {
        TypeDef::UNIT_TYPE
    }
}
impl SpacetimeType for &str {
    fn get_schema() -> TypeDef {
        TypeDef::String
    }
}

impl<T: SpacetimeType> SpacetimeType for Vec<T> {
    fn get_schema() -> TypeDef {
        TypeDef::Builtin(BuiltinType::Array {
            ty: Box::new(T::get_schema()),
        })
    }
}

impl<T: SpacetimeType> SpacetimeType for Option<T> {
    fn get_schema() -> TypeDef {
        TypeDef::make_option_type(T::get_schema())
    }
}

impl SpacetimeType for Hash {
    fn get_schema() -> TypeDef {
        TypeDef::bytes()
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
