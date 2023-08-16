use super::{AlgebraicType, BuiltinType, ProductType, SumType};
use crate::de::fmt_fn;
use fmt_algebraic_type as fmt;
use std::fmt::Display;

/// Wraps the algebraic `ty` into a `Display`able.
///
/// NOTE: You might ask: Why do we have a formatter and a notation for
/// `AlgebraicType`s if we don't have an encoding for `AlgebraicType`s?
///
/// This is because we just want an easier to read text format for algebraic
/// types. This could just as easily take in an algebraic value, which
/// represents an algebraic type and format it that way. It's just more
/// convenient to format it from the Rust type.
pub fn fmt_algebraic_type(ty: &AlgebraicType) -> impl '_ + Display {
    fmt_fn(move |f| match ty {
        AlgebraicType::Sum(ty) => write!(f, "{}", fmt_sum_type(ty)),
        AlgebraicType::Product(ty) => write!(f, "{}", fmt_product_type(ty)),
        AlgebraicType::Builtin(p) => write!(f, "{}", fmt_builtin_type(p)),
        AlgebraicType::Ref(r) => write!(f, "{}", r),
        AlgebraicType::Map(m) => write!(f, "Map<{}, {}>", fmt(&m.key_ty), fmt(&m.ty)),
    })
}

/// Wraps the builtin `ty` into a `Display`able.
fn fmt_product_type(ty: &ProductType) -> impl '_ + Display {
    fmt_fn(move |f| {
        write!(f, "(")?;
        for (i, e) in ty.elements.iter().enumerate() {
            if let Some(name) = &e.name {
                write!(f, "{}", name)?;
            } else {
                write!(f, "{}", i)?;
            }
            write!(f, ": ")?;
            write!(f, "{}", fmt_algebraic_type(&e.algebraic_type))?;
            if i < ty.elements.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    })
}

/// Wraps the builtin `ty` into a `Display`able.
fn fmt_sum_type(ty: &SumType) -> impl '_ + Display {
    fmt_fn(move |f| {
        if ty.variants.is_empty() {
            return write!(f, "(|)");
        }
        write!(f, "(")?;
        for (i, e) in ty.variants.iter().enumerate() {
            if let Some(name) = &e.name {
                write!(f, "{}", name)?;
                write!(f, ": ")?;
            }
            write!(f, "{}", fmt_algebraic_type(&e.algebraic_type))?;
            if i < ty.variants.len() - 1 {
                write!(f, " | ")?;
            }
        }
        write!(f, ")")
    })
}

/// Wraps the builtin `ty` into a `Display`able.
fn fmt_builtin_type(ty: &BuiltinType) -> impl '_ + Display {
    fmt_fn(move |f| match ty {
        BuiltinType::Bool => write!(f, "Bool"),
        BuiltinType::I8 => write!(f, "I8"),
        BuiltinType::U8 => write!(f, "U8"),
        BuiltinType::I16 => write!(f, "I16"),
        BuiltinType::U16 => write!(f, "U16"),
        BuiltinType::I32 => write!(f, "I32"),
        BuiltinType::U32 => write!(f, "U32"),
        BuiltinType::I64 => write!(f, "I64"),
        BuiltinType::U64 => write!(f, "U64"),
        BuiltinType::I128 => write!(f, "I128"),
        BuiltinType::U128 => write!(f, "U128"),
        BuiltinType::F32 => write!(f, "F32"),
        BuiltinType::F64 => write!(f, "F64"),
        BuiltinType::String => write!(f, "String"),
        BuiltinType::Array(a) => write!(f, "Array<{}>", fmt(&a.elem_ty)),
    })
}
