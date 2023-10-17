use crate::{de::fmt_fn, AlgebraicType, BuiltinType::*};
use std::fmt;

/// Wraps an algebraic `ty` in a `Display` impl using a object/map JSON-like notation.
pub fn fmt_algebraic_type(ty: &AlgebraicType) -> impl '_ + fmt::Display {
    use fmt_algebraic_type as fmt;

    // Format name/index + type.
    let fmt_name_ty = |f: &mut fmt::Formatter<'_>, i, name, ty| match name {
        Some(name) => write!(f, "{}: {}", name, fmt(ty)),
        None => write!(f, "{}: {}", i, fmt(ty)),
    };

    fmt_fn(move |f| match ty {
        AlgebraicType::Ref(r) => write!(f, "{{ ty_: Ref, 0: {} }}", r.0),
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
        AlgebraicType::Builtin(Array(ty)) => write!(f, "{{ ty_: Array, 0: {} }}", fmt(&ty.elem_ty)),
        AlgebraicType::Builtin(Map(map)) => write!(f, "{{ ty_: Map, 0: {}, 1: {} }}", fmt(&map.key_ty), fmt(&map.ty)),
        AlgebraicType::Builtin(Bool) => write!(f, "{{ ty_: Bool }}"),
        AlgebraicType::Builtin(I8) => write!(f, "{{ ty_: I8 }}"),
        AlgebraicType::Builtin(U8) => write!(f, "{{ ty_: U8 }}"),
        AlgebraicType::Builtin(I16) => write!(f, "{{ ty_: I16 }}"),
        AlgebraicType::Builtin(U16) => write!(f, "{{ ty_: U16 }}"),
        AlgebraicType::Builtin(I32) => write!(f, "{{ ty_: I32 }}"),
        AlgebraicType::Builtin(U32) => write!(f, "{{ ty_: U32 }}"),
        AlgebraicType::Builtin(I64) => write!(f, "{{ ty_: I64 }}"),
        AlgebraicType::Builtin(U64) => write!(f, "{{ ty_: U64 }}"),
        AlgebraicType::Builtin(I128) => write!(f, "{{ ty_: I128 }}"),
        AlgebraicType::Builtin(U128) => write!(f, "{{ ty_: U128 }}"),
        AlgebraicType::Builtin(F32) => write!(f, "{{ ty_: F32 }}"),
        AlgebraicType::Builtin(F64) => write!(f, "{{ ty_: F64 }}"),
        AlgebraicType::Builtin(String) => write!(f, "{{ ty_: String }}"),
    })
}
