use super::AlgebraicType;
use crate::builtin_type::BuiltinType;
use crate::de::fmt_fn;
use crate::{ArrayType, MapType};
use std::fmt::{self, Formatter};

/// Wraps an algebraic `ty` in a `Display` impl using a object/map JSON-like notation.
pub fn fmt_algebraic_type(ty: &AlgebraicType) -> impl '_ + fmt::Display {
    use fmt_algebraic_type as fmt;

    // Format name/index + type.
    let fmt_name_ty = |f: &mut Formatter<'_>, i, name, ty| match name {
        Some(name) => write!(f, "{}: {}", name, fmt(ty)),
        None => write!(f, "{}: {}", i, fmt(ty)),
    };

    fmt_fn(move |f| match ty {
        AlgebraicType::Sum(ty) => {
            write!(f, "{{ ty_: Sum")?;
            for (i, e_ty) in ty.variants.iter().enumerate() {
                write!(f, ", ")?;
                fmt_name_ty(f, i, e_ty.name.as_deref(), &e_ty.algebraic_type)?;
            }
            write!(f, " }}")
        }
        AlgebraicType::Product(ty) => {
            write!(f, "{{ ty_: Product")?;
            for (i, e_ty) in ty.elements.iter().enumerate() {
                write!(f, ", ")?;
                fmt_name_ty(f, i, e_ty.name.as_deref(), &e_ty.algebraic_type)?;
            }
            write!(f, " }}")
        }
        AlgebraicType::Builtin(ty) => {
            write!(f, "{{ ty_: Builtin")?;
            match &ty {
                BuiltinType::Bool => write!(f, ", 0: Bool")?,
                BuiltinType::I8 => write!(f, ", 0: I8")?,
                BuiltinType::U8 => write!(f, ", 0: U8")?,
                BuiltinType::I16 => write!(f, ", 0: I16")?,
                BuiltinType::U16 => write!(f, ", 0: U16")?,
                BuiltinType::I32 => write!(f, ", 0: I32")?,
                BuiltinType::U32 => write!(f, ", 0: U32")?,
                BuiltinType::I64 => write!(f, ", 0: I64")?,
                BuiltinType::U64 => write!(f, ", 0: U64")?,
                BuiltinType::I128 => write!(f, ", 0: I128")?,
                BuiltinType::U128 => write!(f, ", 0: U128")?,
                BuiltinType::F32 => write!(f, ", 0: F32")?,
                BuiltinType::F64 => write!(f, ", 0: F64")?,
                BuiltinType::String => write!(f, ", 0: String")?,
                BuiltinType::Array(ArrayType { elem_ty }) => write!(f, ", 0: Array, 1: {}", fmt(elem_ty))?,
                BuiltinType::Map(MapType { key_ty, ty }) => write!(f, "0: Map, 1: {}, 2: {}", fmt(key_ty), fmt(ty))?,
            }
            write!(f, " }}")
        }
        AlgebraicType::Ref(r) => write!(f, "{{ ty_: Ref, 0: {} }}", r.0),
    })
}
