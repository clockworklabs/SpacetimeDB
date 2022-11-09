use std::any::TypeId;
use std::mem::ManuallyDrop;

use spacetimedb_lib::{DataKey, Hash};

use super::{PrimaryKey, TypeDef, TypeValue};
use crate::{FilterableValue, FromValue, IntoValue, SchemaType, UniqueValue};

macro_rules! impl_primitives {
    ($($t:ty => $x:ident,)*) => {
        $(
            impl SchemaType for $t {
                fn get_schema() -> TypeDef {
                    TypeDef::$x
                }
            }
            impl FromValue for $t {
                fn from_value(v: TypeValue) -> Option<Self> {
                    match v {
                        TypeValue::$x(v) => Some(v.into()),
                        _ => None,
                    }
                }
            }
            impl IntoValue for $t {
                fn into_value(self) -> TypeValue {
                    TypeValue::$x(self.into())
                }
            }
        )*
    };
    (uniq { $($t:ty => $x:ident,)*}) => {
        impl_primitives!($($t => $x,)*);
        $(
            impl FilterableValue for $t {
                fn equals(&self, other: &TypeValue) -> bool {
                    match other {
                        TypeValue::$x(x) => self == x,
                        _ => false,
                    }
                }
            }
            impl UniqueValue for $t {
                fn into_primarykey(self) -> PrimaryKey {
                    todo!() // idk what this is
                }
            }
        )*
    };
}

impl_primitives! {
    uniq {
        u8 => U8,
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

impl SchemaType for () {
    fn get_schema() -> TypeDef {
        TypeDef::Unit
    }
}
impl FromValue for () {
    fn from_value(v: TypeValue) -> Option<Self> {
        match v {
            TypeValue::Unit => Some(()),
            _ => None,
        }
    }
}
impl IntoValue for () {
    fn into_value(self) -> TypeValue {
        TypeValue::Unit
    }
}
// impl SchemaType for &'_ str {
//     fn get_schema() -> TypeDef {
//         Primitive::String.into()
//     }
// }
// impl IntoValue for &'_ str {
//     fn into_value(self) -> TypeValue {
//         TypeValue::String(self.to_owned())
//     }
// }

impl<T: SchemaType> SchemaType for Vec<T> {
    fn get_schema() -> TypeDef {
        if TypeId::of::<T>() == TypeId::of::<u8>() {
            return TypeDef::Bytes;
        }
        TypeDef::Vec {
            element_type: Box::new(T::get_schema()),
        }
    }
}
impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(v: TypeValue) -> Option<Self> {
        if TypeId::of::<T>() == TypeId::of::<u8>() {
            let bytes = match v {
                TypeValue::Bytes(v) => ManuallyDrop::new(v),
                _ => return None,
            };
            // real specialization is unstable, so, uh
            // SAFETY: we confirmed that Vec<T> is really Vec<u8>
            return Some(unsafe { std::mem::transmute_copy::<Vec<u8>, Vec<T>>(&bytes) });
        }
        let v = match v {
            TypeValue::Vec(v) => v,
            _ => return None,
        };
        v.into_iter().map(T::from_value).collect()
    }
}
impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> TypeValue {
        if TypeId::of::<T>() == TypeId::of::<u8>() {
            // SAFETY: we confirmed that Vec<T> is really Vec<u8>
            let bytes = unsafe {
                let this = ManuallyDrop::new(self);
                std::mem::transmute_copy::<Vec<T>, Vec<u8>>(&this)
            };
            return TypeValue::Bytes(bytes);
        }
        TypeValue::Vec(self.into_iter().map(T::into_value).collect())
    }
}

impl SchemaType for Hash {
    fn get_schema() -> TypeDef {
        TypeDef::Hash
    }
}
impl FromValue for Hash {
    fn from_value(v: TypeValue) -> Option<Self> {
        match v {
            TypeValue::Hash(v) => Some(*v),
            _ => None,
        }
    }
}
impl IntoValue for Hash {
    fn into_value(self) -> TypeValue {
        TypeValue::Hash(Box::new(self))
    }
}
impl FilterableValue for Hash {
    fn equals(&self, other: &TypeValue) -> bool {
        match other {
            TypeValue::Hash(b) => self.data[..] == b.data[..],
            _ => false,
        }
    }
}
impl UniqueValue for Hash {
    fn into_primarykey(self) -> PrimaryKey {
        PrimaryKey {
            data_key: DataKey::Hash(self),
        }
    }
}
